//! hotty-blitz: everything a HOTTY host needs between "a command arrived" and
//! "here are the pixels for this cell rectangle".
//!
//! Two hosts use it: the `hotty run` polyfill (through the Rust API)
//! and the Ghostty fork (PoC-2, through [`ffi`]). Neither knows anything about
//! HTML; both hand commands to a [`Host`] and draw the frames it produces.

mod anim;
pub mod delta;
pub mod fetch;
pub mod ffi;
pub mod input;
pub mod net;
pub mod paint;
pub mod policy;
pub mod style;
mod surface;

use blitz_dom::FontContext;
use hotty_wire::{Command, Control};
use std::collections::BTreeMap;
use std::sync::Arc;

pub use input::{Event, Key, KeyName, KeyOutcome, Mods, PointerKind};
pub use surface::{Damage, Frame, Timings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn parse(s: &str) -> Option<Rgb> {
        // "#rrggbb" or X11 "rgb:rr/gg/bb" (1–4 hex digits per channel).
        if let Some(h) = s.strip_prefix('#') {
            if h.len() == 6 {
                let v = u32::from_str_radix(h, 16).ok()?;
                return Some(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8));
            }
            return None;
        }
        let body = s.strip_prefix("rgb:")?;
        let mut parts = body.split('/');
        let mut chan = || -> Option<u8> {
            let p = parts.next()?;
            let v = u32::from_str_radix(p, 16).ok()?;
            let max = (1u32 << (4 * p.len() as u32)) - 1;
            Some(((v * 255 + max / 2) / max) as u8)
        };
        Some(Rgb(chan()?, chan()?, chan()?))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub fg: Rgb,
    pub bg: Rgb,
    pub palette: [Rgb; 16],
    pub dark: bool,
}

impl Default for Theme {
    fn default() -> Theme {
        // One Dark, which is also the maintainer's Ghostty theme.
        let p = [
            "#282c34", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#abb2bf",
            "#5c6370", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#ffffff",
        ];
        Theme {
            fg: Rgb(0xab, 0xb2, 0xbf),
            bg: Rgb(0x28, 0x2c, 0x34),
            palette: p.map(|c| Rgb::parse(c).unwrap()),
            dark: true,
        }
    }
}

/// The terminal's cell geometry, in device pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub cell_w: u32,
    pub cell_h: u32,
    /// Device pixels per CSS pixel.
    pub scale: f32,
}

impl Default for Metrics {
    fn default() -> Metrics {
        Metrics {
            cell_w: 10,
            cell_h: 21,
            scale: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub metrics: Metrics,
    pub theme: Theme,
    /// The terminal's font family; the host stylesheet makes it the default.
    pub font_family: String,
    /// Root font size in CSS px; derived from the cell height when `None`.
    pub font_size: Option<f32>,
    /// Size limit of the in-band resource store.
    pub resource_quota: usize,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            metrics: Metrics::default(),
            theme: Theme::default(),
            font_family: String::new(),
            font_size: None,
            resource_quota: 64 << 20,
        }
    }
}

/// What the host adapter must do after a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Bytes for the program's input: replies and events.
    Reply(Vec<u8>),
    /// Place `surface` at the current output position (the cursor): the
    /// surface is `cols`×`rows` cells, and the placement shows `window` of
    /// it (SPEC §5.2), over the window's cells, above or below overlapping
    /// placements by `z` (greater above; among equals, the one whose
    /// surface was `created` later above). Its pixels come from
    /// [`Host::render_dirty`] when the surface changed; an adapter that no
    /// longer has them asks again with [`Host::redeliver`].
    Place {
        surface: String,
        cols: u16,
        rows: u16,
        window: Window,
        z: i16,
        created: u32,
        move_cursor: bool,
    },
    /// The surface is hidden (SPEC §5.4): remove its placement. Its document
    /// stays; an adapter that keeps the pixels can show it again without them.
    Hide { surface: String },
    /// The surface is gone; remove its placement.
    Delete { surface: String },
}

/// The part of a surface a placement shows, in cells from its top-left
/// corner (SPEC §5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

pub struct Host {
    config: Config,
    css: String,
    store: Arc<net::Store>,
    font_ctx: FontContext,
    surfaces: BTreeMap<String, surface::Surface>,
    /// Surfaces created so far, for their order (SPEC §5.2).
    created: u32,
    /// A press with Alt held began the gesture under way: it and the rest
    /// of the gesture are the program's, not a surface's (SPEC §9.2).
    program_press: bool,
    /// The embedding terminal hands the pointer through where a surface
    /// does not take it (`takes_pointer`, SPEC §9.3), and says so in the
    /// capabilities.
    passthrough: bool,
    /// `fit` events renders found (SPEC §5.2), at most one per surface:
    /// the rows of the last frame drawn.
    fits: Vec<(String, u16)>,
    frame_log: Option<FrameLog>,
}

/// `HOTTY_FRAME_LOG=<file>`: one tab-separated line per rendered surface,
/// timing the commands applied since the last frame and each render stage,
/// including the host's own handling of the pixels (`deliver`, the callback).
struct FrameLog {
    file: std::fs::File,
    commands: u32,
    handle: std::time::Duration,
}

impl FrameLog {
    const HEADER: &str = "surface\tcommands\thandle_us\tresolve_us\tdiff_us\tpaint_us\tdeliver_us\ttotal_us\tdamaged_px";

    fn open() -> Option<FrameLog> {
        use std::io::Write;
        let path = std::env::var_os("HOTTY_FRAME_LOG")?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()?;
        writeln!(file, "{}", Self::HEADER).ok()?;
        Some(FrameLog {
            file,
            commands: 0,
            handle: std::time::Duration::ZERO,
        })
    }
}

impl Drop for Host {
    /// Fetches still under way outlive the host; its waker does not.
    fn drop(&mut self) {
        self.store.set_waker(None);
    }
}

impl Host {
    pub fn new(config: Config) -> Host {
        // Stylo ships `:has()` behind a pref that Blitz leaves off.
        stylo_static_prefs::set_pref!("layout.css.has-selector.enabled", true);
        let css = style::host_css(&config);
        Host {
            store: net::Store::new(config.resource_quota),
            config,
            css,
            font_ctx: FontContext::default(),
            surfaces: BTreeMap::new(),
            created: 0,
            program_press: false,
            passthrough: false,
            fits: Vec::new(),
            frame_log: FrameLog::open(),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// New cell size, theme or font: every surface restyles and re-renders at
    /// the new pixel size, with no help from the program (PoC-2's promise).
    pub fn set_config(&mut self, config: Config) {
        if config == self.config {
            return;
        }
        let css = style::host_css(&config);
        for s in self.surfaces.values_mut() {
            s.replace_host_css(&self.css, &css);
            s.dirty = true;
        }
        self.css = css;
        self.config = config;
    }

    /// Paint every surface in full on the next render, for a terminal that
    /// lost the images it had (and for checking partial paints against it).
    pub fn repaint(&mut self) {
        for s in self.surfaces.values_mut() {
            s.repaint();
        }
    }

    pub fn surface_names(&self) -> impl Iterator<Item = &str> {
        self.surfaces.keys().map(String::as_str)
    }

    /// Placement of a surface, if it is placed: `(cols, rows)`.
    pub fn placement(&self, name: &str) -> Option<(u16, u16)> {
        self.surfaces
            .get(name)
            .filter(|s| s.placed)
            .map(|s| (s.cols, s.rows))
    }

    /// An element of `surface` as the program wrote it, for the shared
    /// conformance vectors (conformance/README.md).
    pub fn inspect(&self, surface: &str, id: &str) -> Option<serde_json::Value> {
        self.surfaces.get(surface)?.inspect(id)
    }

    /// The centre of element `id` of `surface`, in pixels from the
    /// surface's top left, for [`Host::pointer`]: the test interface's way
    /// to point at an element (SPEC §16). The layout is the last render's.
    pub fn element_centre(&self, surface: &str, id: &str) -> Option<(f32, f32)> {
        let scale = self.config.metrics.scale;
        let (x, y) = self.surfaces.get(surface)?.element_centre(id)?;
        Some((x * scale, y * scale))
    }

    /// Whether `surface` is placed: shown, and so able to take the pointer.
    pub fn is_placed(&self, surface: &str) -> bool {
        self.surfaces.get(surface).is_some_and(|s| s.placed)
    }

    /// The next render delivers `surface`'s whole frame, changed or not: for
    /// an adapter that must show it anew and no longer has its pixels.
    pub fn redeliver(&mut self, surface: &str) {
        if let Some(s) = self.surfaces.get_mut(surface).filter(|s| s.placed) {
            s.redeliver = true;
            s.dirty = true;
        }
    }

    /// Whether [`Host::render_dirty`] has something to render: a placed
    /// surface changed, an animated image on one is due to show its next
    /// frame, or something a document fetched arrived.
    pub fn has_dirty(&self) -> bool {
        let now = std::time::Instant::now();
        self.store.has_delivered()
            || self
                .surfaces
                .values()
                .any(|s| s.placed && (s.dirty || s.next_frame().is_some_and(|t| t <= now)))
    }

    /// When an animated image (GIF, APNG, WebP) on a placed surface is due
    /// to show its next frame: render then. `None` while nothing plays, or
    /// nothing that plays is in a placement's window. Each animated image
    /// has one deadline at a time, so a host needs one timer. Now, when
    /// something a document fetched has arrived.
    pub fn next_frame(&self) -> Option<std::time::Instant> {
        if self.store.has_delivered() {
            return Some(std::time::Instant::now());
        }
        self.surfaces
            .values()
            .filter(|s| s.placed)
            .filter_map(|s| s.next_frame())
            .min()
    }

    /// The host's half of the network policy (SPEC §7.2), in CSP's syntax:
    /// `img-src https://example.com; font-src https:`. A host run by a
    /// person starts with none, and its user grants it. The capabilities
    /// report it (`net`). Fetching happens on threads of its own; set a
    /// waker ([`Host::set_waker`]) to hear when something arrives.
    pub fn set_network(&mut self, policy: &str) {
        self.store.set_host_policy(policy::Policy::parse(policy));
    }

    /// Called on a fetching thread when something a document fetched
    /// arrives (or fails): render on the host's own thread then
    /// ([`Host::has_dirty`] is true).
    pub fn set_waker(&mut self, wake: impl Fn() + Send + Sync + 'static) {
        self.store.set_waker(Some(Arc::new(wake)));
    }

    /// No waker: once this returns, the last one is not called again.
    pub fn clear_waker(&mut self) {
        self.store.set_waker(None);
    }

    /// The fetch limits ([`fetch::MAX_BYTES`], [`fetch::TIMEOUT`]), for
    /// tests that cannot wait for the real ones.
    #[doc(hidden)]
    pub fn set_fetch_limits(&mut self, max_bytes: usize, timeout: std::time::Duration) {
        self.store.set_limits(fetch::Limits { max_bytes, timeout });
    }

    /// Hands the surfaces whose documents fetched something what arrived:
    /// they draw again.
    fn take_network(&mut self) {
        let docs = self.store.take_delivered();
        if docs.is_empty() {
            return;
        }
        for s in self.surfaces.values_mut() {
            if docs.contains(&s.doc_id()) {
                s.net_arrived();
            }
        }
    }

    /// Moves the animated images of placed surfaces to the frames they show
    /// at `now`; the surfaces that changed are dirty. [`Host::render_dirty`]
    /// does it with the current time.
    pub fn animate(&mut self, now: std::time::Instant) {
        for s in self.surfaces.values_mut() {
            if s.placed {
                s.advance(now);
            }
        }
    }

    /// Everything goes: full reset, or the program's session ended.
    pub fn reset(&mut self) -> Vec<Effect> {
        let names: Vec<String> = self.surfaces.keys().cloned().collect();
        names
            .into_iter()
            .filter_map(|n| self.remove_surface(&n))
            .collect()
    }

    pub fn remove_surface(&mut self, name: &str) -> Option<Effect> {
        let s = self.surfaces.remove(name)?;
        self.store.forget_document(s.doc_id());
        s.placed.then(|| Effect::Delete {
            surface: name.to_string(),
        })
    }

    /// Renders every placed surface whose document changed, calling `out`
    /// with its frame and what changed.
    pub fn render_dirty(&mut self, out: &mut dyn FnMut(&str, &Frame, &Damage)) {
        self.take_network();
        self.animate(std::time::Instant::now());
        let metrics = self.config.metrics;
        let dark = self.config.theme.dark;
        for (name, s) in self.surfaces.iter_mut() {
            if !(s.dirty && s.placed) {
                continue;
            }
            let damage = s.render(&metrics, dark);
            if let Some(rows) = s.fit_event.take() {
                match self.fits.iter_mut().find(|(n, _)| n == name) {
                    Some(f) => f.1 = rows,
                    None => self.fits.push((name.clone(), rows)),
                }
            }
            if let Some(damage) = damage {
                let t = std::time::Instant::now();
                out(name, &s.frame, &damage);
                if let Some(log) = &mut self.frame_log {
                    use std::io::Write;
                    let deliver = t.elapsed();
                    let tm = s.frame.timings;
                    let px: u64 = match &damage {
                        Damage::Full => s.frame.width as u64 * s.frame.height as u64,
                        Damage::Rects(rs) => rs.iter().map(|r| r.w as u64 * r.h as u64).sum(),
                    };
                    let handle = log.handle.as_micros() as u64;
                    let total = handle
                        + (tm.resolve_us + tm.diff_us + tm.paint_us) as u64
                        + deliver.as_micros() as u64;
                    let _ = writeln!(
                        log.file,
                        "{name}\t{}\t{handle}\t{}\t{}\t{}\t{}\t{total}\t{px}",
                        log.commands,
                        tm.resolve_us,
                        tm.diff_us,
                        tm.paint_us,
                        deliver.as_micros(),
                    );
                    log.commands = 0;
                    log.handle = std::time::Duration::ZERO;
                }
            }
        }
    }

    /// The events rendering produced, for the program: `fit` (SPEC §5.2),
    /// at most one per surface, with the rows of the last frame drawn.
    /// Take them after [`Host::render_dirty`].
    pub fn take_events(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.fits)
            .into_iter()
            .filter(|(name, _)| {
                self.surfaces
                    .get(name)
                    .is_some_and(|s| s.fit.is_some() && !s.is_detached())
            })
            .map(|(name, rows)| {
                let e = Event {
                    kind: "fit",
                    target: String::new(),
                    detail: serde_json::json!({ "r": rows }),
                };
                Effect::Reply(e.encode(&name))
            })
            .collect()
    }

    pub fn frame(&self, name: &str) -> Option<&Frame> {
        self.surfaces.get(name).map(|s| &s.frame)
    }

    /// The surface's text, one string per terminal row it covers (SPEC §11).
    pub fn text_rows(&self, name: &str) -> Vec<String> {
        let m = self.config.metrics;
        self.surfaces
            .get(name)
            .map(|s| s.text_rows(&m))
            .unwrap_or_default()
    }

    pub fn handle(&mut self, cmd: &Command) -> Vec<Effect> {
        let start = self.frame_log.is_some().then(std::time::Instant::now);
        let effects = self.handle_command(cmd);
        // The animated images the command had documents load start now.
        let now = std::time::Instant::now();
        for s in self.surfaces.values_mut() {
            s.adopt_animations(now);
        }
        if let (Some(start), Some(log)) = (start, &mut self.frame_log) {
            log.commands += 1;
            log.handle += start.elapsed();
        }
        effects
    }

    fn handle_command(&mut self, cmd: &Command) -> Vec<Effect> {
        let mut effects = Vec::new();
        let result = self.dispatch(cmd, &mut effects);
        let quiet: u8 = cmd.get("q").and_then(|q| q.parse().ok()).unwrap_or(0);
        match result {
            Ok(extra) => {
                if quiet == 0 && cmd.action() != "ev" {
                    effects.push(Effect::Reply(reply("ok", cmd, extra.0, extra.1)));
                }
            }
            Err((code, detail)) => {
                if quiet < 2 {
                    let body = serde_json::json!({ "code": code, "detail": detail }).to_string();
                    effects.push(Effect::Reply(reply(
                        "err",
                        cmd,
                        Vec::new(),
                        Some(body.into_bytes()),
                    )));
                }
            }
        }
        effects
    }

    fn dispatch(
        &mut self,
        cmd: &Command,
        effects: &mut Vec<Effect>,
    ) -> Result<ReplyExtra, (&'static str, String)> {
        let surface_name = || -> Result<&str, (&'static str, String)> {
            let s = cmd
                .get("s")
                .ok_or(("EINVAL", "missing s=<surface>".to_string()))?;
            if s.is_empty()
                || s.len() > 64
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return Err(("EINVAL", format!("bad surface name {s:?}")));
            }
            Ok(s)
        };
        match cmd.action() {
            "q" => {
                let m = self.config.metrics;
                let caps = serde_json::json!({
                    "v": "0.1",
                    "host": "hotty-blitz",
                    // Raised with a change a program may need to know of
                    // (SPEC §15): 0.0.2 paints a box resized through a var
                    // delta (Blitz fork 0005); 0.0.3 takes `a=delta`, which
                    // was `a=patch`, and no longer the old name.
                    "version": env!("CARGO_PKG_VERSION"),
                    "ops": ["morph", "inner", "replace", "append", "prepend", "before", "after",
                            "remove", "attr", "unattr", "text", "var"],
                    "events": input::EVENTS,
                    "cell": { "w": m.cell_w, "h": m.cell_h },
                    "scale": m.scale,
                    "scheme": if self.config.theme.dark { "dark" } else { "light" },
                    "limits": { "resources": self.store.quota },
                    // The host's half of the network policy (SPEC §7.2):
                    // empty unless its user granted something.
                    "net": self.store.host_policy().to_json(),
                });
                let mut caps = caps;
                if self.passthrough {
                    caps["passthrough"] = serde_json::Value::Bool(true);
                }
                Ok((Vec::new(), Some(caps.to_string().into_bytes())))
            }
            "doc" => {
                let name = surface_name()?.to_string();
                let html = cmd.payload_str().map_err(|e| ("EINVAL", e))?;
                let detached = cmd.get("d") == Some("1");
                let (cols, rows, auto, placed, presses, fit, hover, created, window) =
                    match self.surfaces.remove(&name) {
                        Some(mut old) => {
                            self.store.forget_document(old.doc_id());
                            // A new document ends a drag under way (SPEC §9.1),
                            // unless it detaches the surface (§5.5).
                            if let Some(e) = old.cancel_drag(Mods::default())
                                && !detached
                            {
                                effects.push(Effect::Reply(e.encode(&name)));
                            }
                            (
                                old.cols,
                                old.rows,
                                old.auto_rows,
                                old.placed,
                                old.presses,
                                old.fit,
                                old.hover,
                                old.created,
                                old.window,
                            )
                        }
                        None => {
                            self.created += 1;
                            (80, 24, false, false, false, None, None, self.created, None)
                        }
                    };
                let mut s = surface::Surface::new(
                    html,
                    &self.config,
                    &self.css,
                    self.store.clone(),
                    self.font_ctx.clone(),
                    (cols, rows),
                );
                // A replaced document keeps its placement: `r=auto` is
                // measured when placed, not on every change.
                s.placed = placed;
                s.window = window;
                s.presses = presses;
                s.fit = fit;
                // `hover` goes on from what the program last heard (SPEC §9.4).
                s.hover = hover;
                s.auto_rows = auto;
                s.rows = rows;
                s.created = created;
                // `d=1`: detached from the start (SPEC §5.5). Without it, the
                // surface is the program's again, detached before or not.
                if detached {
                    s.detach();
                }
                self.surfaces.insert(name, s);
                Ok((Vec::new(), None))
            }
            "place" => {
                let name = surface_name()?.to_string();
                let s = self
                    .surfaces
                    .get_mut(&name)
                    .ok_or(("ENOENT", format!("no surface {name}")))?;
                let cols: u16 = cmd
                    .get("c")
                    .ok_or(("EINVAL", "place needs c=<cols>".to_string()))?
                    .parse()
                    .map_err(|_| ("EINVAL", "bad c".to_string()))?;
                if cols == 0 || cols > 1000 {
                    return Err(("EINVAL", "c out of range".into()));
                }
                let auto = cmd.get("r").is_none_or(|r| r == "auto");
                let rows: u16 = if auto {
                    s.content_rows(&self.config.metrics, cols)
                } else {
                    cmd.get("r")
                        .unwrap()
                        .parse()
                        .map_err(|_| ("EINVAL", "bad r".to_string()))?
                };
                if rows == 0 || rows > 1000 {
                    return Err(("EINVAL", "r out of range".into()));
                }
                let window = window(cmd, cols, rows)?;
                let z: i16 = match cmd.get("z") {
                    None => 0,
                    Some(v) => v
                        .parse()
                        .ok()
                        .filter(|z| (-1000..=1000).contains(z))
                        .ok_or(("EINVAL", "z is an integer from -1000 to 1000".to_string()))?,
                };
                s.set_size(cols, rows);
                s.auto_rows = auto;
                s.placed = true;
                // A drag goes on across a new placement (SPEC §9.1).
                s.window = Some(window);
                // Like z, the placement's: placing again without it stops them.
                s.presses = cmd.get("p") == Some("1");
                // `fit` starts from the placement's own rows (SPEC §5.2).
                s.fit = (cmd.get("f") == Some("1")).then_some(rows);
                // `hover` goes on from what the program last heard, or
                // starts from out (SPEC §9.4).
                s.hover = (cmd.get("v") == Some("1"))
                    .then(|| s.hover.take().unwrap_or(surface::Hovered::Out));
                s.dirty = true;
                effects.push(Effect::Place {
                    surface: name,
                    cols,
                    rows,
                    window,
                    z,
                    created: s.created,
                    move_cursor: cmd.get("C") != Some("1"),
                });
                Ok((
                    vec![
                        ("c".into(), cols.to_string()),
                        ("r".into(), rows.to_string()),
                    ],
                    None,
                ))
            }
            "delta" => {
                let name = surface_name()?.to_string();
                let s = self
                    .surfaces
                    .get_mut(&name)
                    .ok_or(("ENOENT", format!("no surface {name}")))?;
                let payload = cmd.payload_str().map_err(|e| ("EINVAL", e))?;
                let op = cmd.get("op").unwrap_or("morph");
                let res = s.delta(op, cmd.get("t"), cmd.get("k"), payload);
                s.dirty = true;
                res.map(|_| (Vec::new(), None))
                    .map_err(|e| (e.code, e.detail))
            }
            "res" => {
                let id = cmd
                    .get("id")
                    .ok_or(("EINVAL", "res needs id=<name>".to_string()))?;
                let mime = cmd.get("type").unwrap_or("application/octet-stream");
                let users = self
                    .store
                    .put(id, mime, cmd.payload.clone())
                    .map_err(|e| ("EQUOTA", e))?;
                let href = format!("cid:{id}");
                for s in self.surfaces.values_mut() {
                    if users.contains(&s.doc_id()) {
                        s.resource_changed(&href);
                    }
                }
                Ok((Vec::new(), None))
            }
            "del" => {
                if let Some(id) = cmd.get("id") {
                    self.store.remove(id);
                    return Ok((Vec::new(), None));
                }
                match cmd.get("s") {
                    Some(name) => {
                        let name = name.to_string();
                        if !self.surfaces.contains_key(&name) {
                            return Err(("ENOENT", format!("no surface {name}")));
                        }
                        effects.extend(self.remove_surface(&name));
                    }
                    None => effects.extend(self.reset()),
                }
                Ok((Vec::new(), None))
            }
            "hide" => {
                let name = surface_name()?.to_string();
                let s = self
                    .surfaces
                    .get_mut(&name)
                    .ok_or(("ENOENT", format!("no surface {name}")))?;
                // The keyboard goes back to the terminal, as with a=blur.
                if s.has_focus() {
                    for e in s.blur() {
                        effects.push(Effect::Reply(e.encode(&name)));
                    }
                }
                // Its placement goes away, and with it a drag (SPEC §9.1).
                if let Some(e) = s.cancel_drag(Mods::default()) {
                    effects.push(Effect::Reply(e.encode(&name)));
                }
                if s.placed {
                    s.placed = false;
                    s.presses = false;
                    s.fit = None;
                    s.hover = None;
                    s.window = None;
                    effects.push(Effect::Hide { surface: name });
                }
                Ok((Vec::new(), None))
            }
            "detach" => {
                let name = surface_name()?.to_string();
                let s = self
                    .surfaces
                    .get_mut(&name)
                    .ok_or(("ENOENT", format!("no surface {name}")))?;
                s.detach();
                Ok((Vec::new(), None))
            }
            "focus" | "blur" => {
                let name = surface_name()?.to_string();
                let s = self
                    .surfaces
                    .get_mut(&name)
                    .ok_or(("ENOENT", format!("no surface {name}")))?;
                if cmd.action() == "focus" && s.is_detached() {
                    return Err(("EDETACHED", format!("surface {name} is detached")));
                }
                if cmd.action() == "focus" {
                    s.focus(cmd.get("t")).map_err(|e| ("ENOTARGET", e))?;
                } else {
                    for e in s.blur() {
                        effects.push(Effect::Reply(e.encode(&name)));
                    }
                }
                s.dirty = true;
                Ok((Vec::new(), None))
            }
            "" => Err(("EINVAL", "missing a=<action>".into())),
            other => Err(("EINVAL", format!("unknown action a={other}"))),
        }
    }

    /// A pointer event at pixel position `(x, y)` inside `surface`. Returns
    /// events for the program; hover and pressed styles change locally.
    /// `Down` and `Up` are the primary button's, or a tap's: a click (SPEC
    /// §10.1). A host passes no other button.
    ///
    /// A press on an element with `drag` in its `data-on` starts a drag
    /// (SPEC §9.1), and the host then holds the pointer for `surface` until
    /// the release: every move goes here, wherever it is, with `(x, y)`
    /// counted from the surface's top left even outside it (negative, or
    /// past its size). A host that loses the pointer before the release
    /// sends `Leave`, which ends the drag. Mouse and pen only: a host passes
    /// a touch as a tap (`Down` and `Up` where it lifts), never its moves.
    ///
    /// A press with Alt held is the program's (SPEC §9.2): it and the rest
    /// of its gesture, to the release, reach no surface. The host reports
    /// them to the program as it would over the cells; passing them here is
    /// harmless, and gives the keyboard back as such a press does.
    pub fn pointer(
        &mut self,
        surface: &str,
        kind: PointerKind,
        x: f32,
        y: f32,
        mods: Mods,
    ) -> Vec<Effect> {
        // A press begins a gesture, and decides whose it is: a release the
        // host lost does not leave the next one the program's.
        if kind == PointerKind::Down {
            self.program_press = mods.alt;
        }
        if self.program_press {
            return self.program_pointer(surface, kind, mods);
        }
        let m = self.config.metrics;
        if !self.surfaces.contains_key(surface) {
            return Vec::new();
        }
        let s = self.surfaces.get_mut(surface).expect("checked above");
        // The cell of the surface the pointer is in, outside it too.
        let cell = (
            (x / m.cell_w.max(1) as f32).floor() as i32,
            (y / m.cell_h.max(1) as f32).floor() as i32,
        );
        let (lead, events) = s.pointer_input(kind, x / m.scale, y / m.scale, cell, mods);
        // Where the pointer is now, for `hover` (SPEC §9.4): not while a
        // button is held, and at the release after everything it caused.
        let hovered = match kind {
            PointerKind::Down => None,
            PointerKind::Move if s.held() => None,
            _ => s.hovered(cell, self.passthrough),
        };
        // `press` comes before everything the press causes, on this
        // surface or another (SPEC §9), then a drag's events (§9.1).
        let mut effects: Vec<Effect> = match kind {
            PointerKind::Down => s.pressed().map(|e| Effect::Reply(e.encode(surface))),
            _ => None,
        }
        .into_iter()
        .chain(lead.into_iter().map(|e| Effect::Reply(e.encode(surface))))
        .collect();
        if kind == PointerKind::Down {
            // A press on this surface is outside every other one: any other
            // that has the keyboard gives it back (SPEC §10.1).
            for (name, other) in self.surfaces.iter_mut() {
                if name != surface && other.has_focus() {
                    effects.extend(
                        other
                            .blur()
                            .into_iter()
                            .map(|e| Effect::Reply(e.encode(name))),
                    );
                }
            }
        }
        effects.extend(
            events
                .into_iter()
                .chain(hovered)
                .map(|e| Effect::Reply(e.encode(surface))),
        );
        effects
    }

    /// A pointer event of a gesture that a press with Alt began (SPEC §9.2):
    /// the press is one on the cells, so the surface under it is no longer
    /// hovered and a surface with the keyboard gives it back; nothing else
    /// reaches a surface until the release.
    fn program_pointer(&mut self, surface: &str, kind: PointerKind, mods: Mods) -> Vec<Effect> {
        let mut effects = Vec::new();
        match kind {
            PointerKind::Down => {
                if let Some(s) = self.surfaces.get_mut(surface) {
                    let (lead, events) =
                        s.pointer_input(PointerKind::Leave, 0.0, 0.0, (0, 0), mods);
                    // The gesture hovers nothing: the window is left (§9.4).
                    let out = s.hovered((0, 0), self.passthrough);
                    effects.extend(
                        lead.into_iter()
                            .chain(events)
                            .chain(out)
                            .map(|e| Effect::Reply(e.encode(surface))),
                    );
                }
                for (name, s) in self.surfaces.iter_mut() {
                    if s.has_focus() {
                        effects.extend(s.blur().into_iter().map(|e| Effect::Reply(e.encode(name))));
                    }
                }
            }
            PointerKind::Up => self.program_press = false,
            PointerKind::Move | PointerKind::Leave => {}
        }
        effects
    }

    /// A key for the focused element of `surface`. `consumed` is false when
    /// the element has no use for it; the host then forwards it to the program.
    pub fn key(&mut self, surface: &str, key: &Key) -> KeyOutcome {
        let Some(s) = self.surfaces.get_mut(surface) else {
            return KeyOutcome::default();
        };
        let (consumed, events) = s.key(key);
        KeyOutcome {
            consumed,
            effects: events
                .into_iter()
                .map(|e| Effect::Reply(e.encode(surface)))
                .collect(),
        }
    }

    /// Takes the keyboard away from `surface` (the user clicked elsewhere).
    pub fn blur(&mut self, surface: &str) -> Vec<Effect> {
        let Some(s) = self.surfaces.get_mut(surface) else {
            return Vec::new();
        };
        let events = s.blur();
        events
            .into_iter()
            .map(|e| Effect::Reply(e.encode(surface)))
            .collect()
    }

    /// Whether `surface` currently holds the keyboard.
    pub fn is_focused(&self, surface: &str) -> bool {
        self.surfaces.get(surface).is_some_and(|s| s.has_focus())
    }

    /// The pointer's shape over `surface` (Surface::cursor), for a host
    /// that shows the pointer: a CSS `cursor` name.
    /// The terminal embedding this host lets the pointer pass through where
    /// a surface does not take it (SPEC §9.3): the capabilities say so.
    pub fn set_passthrough(&mut self, on: bool) {
        self.passthrough = on;
    }

    /// Whether `surface` takes the pointer at device pixel (`x`, `y`) of the
    /// surface. False where only boxes with `pointer-events: none` are, and
    /// for a surface there is no such: the host then hands the pointer to
    /// what is below the surface, another placement or the cells (SPEC §9.3).
    pub fn takes_pointer(&mut self, surface: &str, x: f32, y: f32) -> bool {
        let scale = self.config.metrics.scale;
        self.surfaces
            .get_mut(surface)
            .is_some_and(|s| s.takes_pointer(x / scale, y / scale))
    }

    pub fn cursor(&self, surface: &str) -> Option<&'static str> {
        self.surfaces.get(surface)?.cursor()
    }

    /// The hyperlink under the pointer on `surface` (Surface::hyperlink),
    /// for a host to treat as an OSC 8 hyperlink: its url.
    pub fn hyperlink(&self, surface: &str) -> Option<String> {
        self.surfaces.get(surface)?.hyperlink()
    }

    pub fn focused_surface(&self) -> Option<&str> {
        self.surfaces
            .iter()
            .find(|(_, s)| s.has_focus())
            .map(|(n, _)| n.as_str())
    }
}

type ReplyExtra = (Vec<(String, String)>, Option<Vec<u8>>);

fn reply(
    kind: &str,
    cmd: &Command,
    extra: Vec<(String, String)>,
    body: Option<Vec<u8>>,
) -> Vec<u8> {
    let mut control = Control::default();
    control.set("a", kind);
    for key in ["n", "s"] {
        if let Some(v) = cmd.get(key) {
            control.set(key, v);
        }
    }
    control.set("re", cmd.action());
    for (k, v) in extra {
        control.set(&k, v);
    }
    hotty_wire::encode_plain(&control, body.as_deref().unwrap_or_default())
}

/// A place command's window (SPEC §5.2): `x`, `y`, `w`, `h` in cells, by
/// default the whole surface. One that leaves the surface, or has no cells,
/// is EINVAL.
fn window(cmd: &Command, cols: u16, rows: u16) -> Result<Window, (&'static str, String)> {
    let get = |k: &str, default: u16| -> Result<u16, (&'static str, String)> {
        cmd.get(k)
            .map(|v| v.parse::<u16>().map_err(|_| ("EINVAL", format!("bad {k}"))))
            .unwrap_or(Ok(default))
    };
    let (x, y) = (get("x", 0)?, get("y", 0)?);
    let w = get("w", cols.saturating_sub(x))?;
    let h = get("h", rows.saturating_sub(y))?;
    if w == 0 || h == 0 || x as u32 + w as u32 > cols as u32 || y as u32 + h as u32 > rows as u32 {
        return Err(("EINVAL", "the window is not inside the surface".into()));
    }
    Ok(Window { x, y, w, h })
}

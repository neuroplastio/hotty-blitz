//! hotty-blitz: everything a HOTTY host needs between "a command arrived" and
//! "here are the pixels for this cell rectangle".
//!
//! Two hosts use it: the `hotty run` polyfill (through the Rust API)
//! and the Ghostty fork (PoC-2, through [`ffi`]). Neither knows anything about
//! HTML; both hand commands to a [`Host`] and draw the frames it produces.

pub mod ffi;
pub mod input;
pub mod net;
pub mod paint;
pub mod patch;
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
    /// it (SPEC §5.2), over the window's cells. Its pixels come from
    /// [`Host::render_dirty`] when the surface changed; an adapter that no
    /// longer has them asks again with [`Host::redeliver`].
    Place {
        surface: String,
        cols: u16,
        rows: u16,
        window: Window,
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

    /// The next render delivers `surface`'s whole frame, changed or not: for
    /// an adapter that must show it anew and no longer has its pixels.
    pub fn redeliver(&mut self, surface: &str) {
        if let Some(s) = self.surfaces.get_mut(surface).filter(|s| s.placed) {
            s.redeliver = true;
            s.dirty = true;
        }
    }

    pub fn has_dirty(&self) -> bool {
        self.surfaces.values().any(|s| s.dirty && s.placed)
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
        let metrics = self.config.metrics;
        let dark = self.config.theme.dark;
        for (name, s) in self.surfaces.iter_mut() {
            if !(s.dirty && s.placed) {
                continue;
            }
            if let Some(damage) = s.render(&metrics, dark) {
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
                    "ops": ["morph", "inner", "replace", "append", "prepend", "before", "after",
                            "remove", "attr", "unattr", "text", "var"],
                    "events": input::EVENTS,
                    "cell": { "w": m.cell_w, "h": m.cell_h },
                    "scale": m.scale,
                    "scheme": if self.config.theme.dark { "dark" } else { "light" },
                    "limits": { "resources": self.store.quota },
                    // No network (SPEC §7.2): this host fetches nothing but
                    // cid: resources and data: URLs.
                    "net": {},
                });
                Ok((Vec::new(), Some(caps.to_string().into_bytes())))
            }
            "doc" => {
                let name = surface_name()?.to_string();
                let html = cmd.payload_str().map_err(|e| ("EINVAL", e))?;
                let (cols, rows, auto, placed) = self
                    .surfaces
                    .get(&name)
                    .map(|s| (s.cols, s.rows, s.auto_rows, s.placed))
                    .unwrap_or((80, 24, false, false));
                if let Some(old) = self.surfaces.remove(&name) {
                    self.store.forget_document(old.doc_id());
                }
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
                s.auto_rows = auto;
                s.rows = rows;
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
                s.set_size(cols, rows);
                s.auto_rows = auto;
                s.placed = true;
                s.dirty = true;
                effects.push(Effect::Place {
                    surface: name,
                    cols,
                    rows,
                    window,
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
            "patch" => {
                let name = surface_name()?.to_string();
                let s = self
                    .surfaces
                    .get_mut(&name)
                    .ok_or(("ENOENT", format!("no surface {name}")))?;
                let payload = cmd.payload_str().map_err(|e| ("EINVAL", e))?;
                let op = cmd.get("op").unwrap_or("morph");
                let res = s.patch(op, cmd.get("t"), cmd.get("k"), payload);
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
                if s.placed {
                    s.placed = false;
                    effects.push(Effect::Hide { surface: name });
                }
                Ok((Vec::new(), None))
            }
            "focus" | "blur" => {
                let name = surface_name()?.to_string();
                let s = self
                    .surfaces
                    .get_mut(&name)
                    .ok_or(("ENOENT", format!("no surface {name}")))?;
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
    pub fn pointer(
        &mut self,
        surface: &str,
        kind: PointerKind,
        x: f32,
        y: f32,
        mods: Mods,
    ) -> Vec<Effect> {
        let scale = self.config.metrics.scale;
        let Some(s) = self.surfaces.get_mut(surface) else {
            return Vec::new();
        };
        let events = s.pointer(kind, x / scale, y / scale, mods);
        events
            .into_iter()
            .map(|e| Effect::Reply(e.encode(surface)))
            .collect()
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
    hotty_wire::encode(&control, body.as_deref().unwrap_or_default())
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

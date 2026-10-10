//! hotty-blitz: everything a HOTTY host needs between "a command arrived" and
//! "here are the pixels for this cell rectangle".
//!
//! Two hosts use it: the `hotty run` polyfill (through the Rust API)
//! and the Ghostty fork (PoC-2, through [`ffi`]). Neither knows anything about
//! HTML; both hand commands to a [`Host`] and draw the frames it produces.

mod anim;
pub mod delta;
mod edit;
pub mod fetch;
pub mod ffi;
pub mod input;
pub mod net;
pub mod paint;
pub mod policy;
mod scroll;
pub mod style;
mod surface;

use blitz_dom::FontContext;
use hotty_wire::{Command, Control};
use std::collections::BTreeMap;
use std::sync::Arc;

pub use input::{
    Event, Key, KeyName, KeyOutcome, Mods, PointerKind, Touch, TouchOutcome, TouchPhase,
    WheelOutcome,
};
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
    /// Render contexts the surfaces share (surface.rs).
    renderers: surface::Renderers,
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
    /// Surfaces whose size in CSS pixels changed with the terminal's font,
    /// owed a `resize` (SPEC §5.3, §9).
    resizes: Vec<String>,
    /// The wheel gesture under way (SPEC §5.3): where its first wheel sent
    /// it, which the rest of it follows.
    gesture: Option<Gesture>,
    /// The touch under way (Host::touch).
    finger: Option<Finger>,
    frame_log: Option<FrameLog>,
    /// The surface whose document the host is working in (empty: none),
    /// so that a panic there costs that surface alone ([`Host::recover`]).
    working: String,
    /// What surfaces lost to a panic leave the embedder to do: delete
    /// their pictures. [`Host::take_events`] hands them over.
    lost: Vec<Effect>,
}

/// Notes that the host now works in `surface`'s document ([`Host::recover`]).
fn enter(working: &mut String, surface: &str) {
    working.clear();
    working.push_str(surface);
}

/// A touch under way (SPEC §9.1): the surface it began on, where, with
/// which keys held, the pans the touched element allows while it may still
/// drag, whose it is so far, and whether it can still be a tap (it has not
/// gone past the slop, and is no long press).
struct Finger {
    surface: String,
    start: (f32, f32),
    mods: Mods,
    pans: (bool, bool),
    touch: Touch,
    tap: bool,
}

/// How far a finger moves, in CSS pixels, before its touch is a pan or a
/// drag rather than a tap: GTK's drag threshold, and xterm-addon-hotty's.
pub const TAP_SLOP: f32 = 8.0;

/// A wheel gesture: a wheel's notches, or a touchpad's scroll and its
/// momentum, as long as they come within [`GESTURE`] of each other.
struct Gesture {
    surface: String,
    route: scroll::Route,
    at: std::time::Instant,
}

/// A wheel this soon after the one before belongs to the same gesture, as
/// xterm-addon-hotty has it.
const GESTURE: std::time::Duration = std::time::Duration::from_millis(150);

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
            renderers: surface::Renderers::default(),
            created: 0,
            program_press: false,
            passthrough: false,
            fits: Vec::new(),
            resizes: Vec::new(),
            gesture: None,
            finger: None,
            frame_log: FrameLog::open(),
            working: String::new(),
            lost: Vec::new(),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// New cell size, theme or font: every surface restyles and re-renders at
    /// the new pixel size, with no help from the program (PoC-2's promise).
    /// A zoom (SPEC §5.3) changes `scale` with the cells, so a cell keeps its
    /// size in CSS pixels and the layout stays. A new font changes it: each
    /// placed surface is laid out again and owes the program a `resize`.
    pub fn set_config(&mut self, config: Config) {
        if config == self.config {
            return;
        }
        if font_resizes(&self.config, &config) {
            for (name, s) in &self.surfaces {
                if s.placed && !s.is_detached() && !self.resizes.contains(name) {
                    self.resizes.push(name.clone());
                }
            }
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

    /// Text field `id` of `surface`: its value and where its caret is, a
    /// count of characters (grapheme clusters). The test interface's
    /// (SPEC §16), for the `edit` vectors.
    #[doc(hidden)]
    pub fn text_field(&self, surface: &str, id: &str) -> Option<(String, usize)> {
        self.surfaces.get(surface)?.text_field(id)
    }

    /// Where text field `id` of `surface` has its selection's anchor, a
    /// count of characters: its caret when nothing is selected. The test
    /// interface's (SPEC §16).
    #[doc(hidden)]
    pub fn text_anchor(&self, surface: &str, id: &str) -> Option<usize> {
        self.surfaces.get(surface)?.text_anchor(id)
    }

    /// Sets text field `id` of `surface`: its value, and its selection's
    /// anchor and its caret, counts of characters (equal: nothing
    /// selected). The test interface's (SPEC §16).
    #[doc(hidden)]
    pub fn set_text_field(
        &mut self,
        surface: &str,
        id: &str,
        value: &str,
        anchor: usize,
        caret: usize,
    ) -> bool {
        enter(&mut self.working, surface);
        self.surfaces
            .get_mut(surface)
            .is_some_and(|s| s.set_text_field(id, value, anchor, caret))
    }

    /// Does an action of SPEC §10.2 in the focused text field of `surface`,
    /// as a key bound to it does; with `extend`, as the key with Shift
    /// does. The test interface's (SPEC §16).
    #[doc(hidden)]
    pub fn text_action(&mut self, surface: &str, action: &str, extend: bool) -> Vec<Effect> {
        enter(&mut self.working, surface);
        let Some(s) = self.surfaces.get_mut(surface) else {
            return Vec::new();
        };
        s.text_action(action, extend)
            .into_iter()
            .map(|e| Effect::Reply(e.encode(surface)))
            .collect()
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
        enter(&mut self.working, surface);
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
        // Lost surfaces' deletions go out with the next frame's events.
        !self.lost.is_empty()
            || self.store.has_delivered()
            || self
                .surfaces
                .values()
                .any(|s| s.placed && (s.dirty || s.next_frame().is_some_and(|t| t <= now)))
    }

    /// When an animated image (GIF, APNG, WebP) on a placed surface is due
    /// to show its next frame, or scrollbars that fade are drawn again:
    /// render then. `None` while nothing plays, or nothing that plays is in
    /// a placement's window, and no scrollbar fades. Each animated image
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
        for (name, s) in self.surfaces.iter_mut() {
            if docs.contains(&s.doc_id()) {
                enter(&mut self.working, name);
                s.net_arrived();
            }
        }
    }

    /// Moves the animated images of placed surfaces to the frames they show
    /// at `now`; the surfaces that changed are dirty. [`Host::render_dirty`]
    /// does it with the current time.
    pub fn animate(&mut self, now: std::time::Instant) {
        for (name, s) in self.surfaces.iter_mut() {
            if s.placed {
                enter(&mut self.working, name);
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

    /// The host is in no surface's document until a call enters one.
    pub(crate) fn leave(&mut self) {
        self.working.clear();
    }

    /// After a panic: the surface the host was working in goes, as if the
    /// program had deleted it, since its document may be broken; the
    /// others stay. A panic in no surface's document takes them all, as a
    /// full reset would. The program hears nothing (the protocol has no
    /// event for it); what it sends the lost surfaces gets ENOENT. Returns
    /// the surface lost, if it was one.
    pub(crate) fn recover(&mut self) -> Option<String> {
        let name = std::mem::take(&mut self.working);
        self.gesture = None;
        self.program_press = false;
        if self.surfaces.contains_key(&name) {
            let deleted = self.remove_surface(&name);
            self.lost.extend(deleted);
            Some(name)
        } else {
            let all = self.reset();
            self.lost.extend(all);
            None
        }
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
            enter(&mut self.working, name);
            let damage = s.render(&metrics, dark, &mut self.renderers);
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

    /// The events rendering produced, for the program: `resize` (SPEC §9)
    /// after the terminal's font changed, with the surface's size in CSS
    /// pixels now, and `fit` (SPEC §5.2), at most one each per surface,
    /// with the rows of the last frame drawn. Take them after
    /// [`Host::render_dirty`].
    pub fn take_events(&mut self) -> Vec<Effect> {
        let lost = std::mem::take(&mut self.lost);
        let m = self.config.metrics;
        let resizes: Vec<Effect> = std::mem::take(&mut self.resizes)
            .into_iter()
            .filter_map(|name| {
                let s = self.surfaces.get(&name)?;
                if !s.placed || s.is_detached() {
                    return None;
                }
                let (w, h) = css_cell(&m);
                // To a hundredth of a CSS pixel: no f32 noise on the wire.
                let px = |v: f32| (f64::from(v) * 100.0).round() / 100.0;
                let e = Event {
                    kind: "resize",
                    target: String::new(),
                    detail: serde_json::json!({ "w": px(w * s.cols as f32), "h": px(h * s.rows as f32) }),
                };
                Some(Effect::Reply(e.encode(&name)))
            })
            .collect();
        let fits = std::mem::take(&mut self.fits)
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
            });
        lost.into_iter().chain(resizes).chain(fits).collect()
    }

    pub fn frame(&self, name: &str) -> Option<&Frame> {
        self.surfaces.get(name).map(|s| &s.frame)
    }

    /// The resolved value of the CSS `property` on the first element of
    /// surface `name` that `selector` matches, as getComputedStyle gives
    /// it, or on its `::selection` with `selection` (`color` and
    /// `background-color`). Empty when there is none. For tests of the
    /// host stylesheet (SPEC §8): the shared vectors check the wire, not
    /// styles.
    #[doc(hidden)]
    pub fn computed_style(
        &self,
        name: &str,
        selector: &str,
        property: &str,
        selection: bool,
    ) -> String {
        self.surfaces
            .get(name)
            .map(|s| s.computed_style(selector, property, selection))
            .unwrap_or_default()
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
        enter(&mut self.working, cmd.get("s").unwrap_or_default());
        let effects = self.handle_command(cmd);
        // The animated images the command had documents load start now.
        let now = std::time::Instant::now();
        for (name, s) in self.surfaces.iter_mut() {
            enter(&mut self.working, name);
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
                // A document can ask to scroll (SPEC §5.3).
                caps["scroll"] = serde_json::Value::Bool(true);
                // A drag says where in an element with `data-steps` the
                // pointer is (SPEC §9.1).
                caps["steps"] = serde_json::Value::Bool(true);
                Ok((Vec::new(), Some(caps.to_string().into_bytes())))
            }
            "doc" => {
                let name = surface_name()?.to_string();
                let html = cmd.payload_str().map_err(|e| ("EINVAL", e))?;
                let detached = cmd.get("d") == Some("1");
                // A gesture latched to the old document's boxes ends with it.
                if self.gesture.as_ref().is_some_and(|g| g.surface == name) {
                    self.gesture = None;
                }
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
                    // Like `d`, the document's: without it, it does not
                    // scroll (SPEC §5.1).
                    scroll::axes(cmd.get("scroll")),
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
                    for e in s.focus(cmd.get("t")).map_err(|e| ("ENOTARGET", e))? {
                        effects.push(Effect::Reply(e.encode(&name)));
                    }
                    s.dirty = true;
                    // There is one keyboard: a surface that had it gives it
                    // back, as for a click on another surface (SPEC §10.1).
                    for (other_name, other) in self.surfaces.iter_mut() {
                        if *other_name != name && other.has_focus() {
                            for e in other.blur() {
                                effects.push(Effect::Reply(e.encode(other_name)));
                            }
                            other.dirty = true;
                        }
                    }
                } else {
                    for e in s.blur() {
                        effects.push(Effect::Reply(e.encode(&name)));
                    }
                    s.dirty = true;
                }
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
    /// a touch, a tap too, to [`Host::touch`].
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
        self.pointer_with(surface, kind, x, y, mods, true)
    }

    /// [`Host::pointer`], reporting `hover` only when `hover` is true: a
    /// touch's drag hovers nothing (SPEC §9.4).
    fn pointer_with(
        &mut self,
        surface: &str,
        kind: PointerKind,
        x: f32,
        y: f32,
        mods: Mods,
        hover: bool,
    ) -> Vec<Effect> {
        enter(&mut self.working, surface);
        // A press begins a gesture, and decides whose it is: a release the
        // host lost does not leave the next one the program's.
        if kind == PointerKind::Down {
            self.program_press = mods.alt;
        }
        // A press, or the pointer leaving, ends a wheel gesture.
        if matches!(kind, PointerKind::Down | PointerKind::Leave) {
            self.gesture = None;
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
            _ if !hover => None,
            PointerKind::Down => None,
            PointerKind::Move if s.held() => None,
            _ => s.hovered(cell, self.passthrough),
        };
        // `press` comes before everything the press causes, on this
        // surface or another (SPEC §9), then a drag's events (§9.1).
        // A press on a scrollbar's thumb is not the document's.
        let mut effects: Vec<Effect> = match kind {
            PointerKind::Down if !s.on_scrollbar() => {
                s.pressed().map(|e| Effect::Reply(e.encode(surface)))
            }
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

    /// A finger on `surface`, the one its touch began on, at device pixel
    /// (`x`, `y`) of it, counted from its top left even outside it. `mods`
    /// as for a pointer event. A terminal passes every phase of a touch that
    /// begins over a surface here, and none of it to [`Host::pointer`].
    ///
    /// A touch drags an element that opts in to drags when the
    /// `touch-action` of the touched element allows no pan along the
    /// touch's first move past [`TAP_SLOP`] (SPEC §9.1); one that never
    /// goes past it is a tap. hotty-blitz decides, and says whose the touch
    /// is:
    /// - `Down`: [`Touch::Undecided`] when it may drag, and the terminal
    ///   then holds its moves; [`Touch::Terminal`] otherwise (no such
    ///   element under it, Alt held, the surface detached or not placed),
    ///   and the terminal may scroll with its moves.
    /// - `Move`: `Undecided` until the finger has gone past the slop. Then
    ///   [`Touch::Surface`] if it drags, and every move after is the
    ///   surface's, wherever the finger goes; or `Terminal`, a pan: the
    ///   terminal scrolls with it from where it began, as with any touch
    ///   ([`Host::wheel`] first).
    /// - `Up`: `Surface` when the surface took the touch: the end of a
    ///   drag, or a tap, which it takes as a click where the finger lifted
    ///   (a press there, never a drag). `Terminal` after a pan or a long
    ///   press.
    /// - `Cancel` (a second finger, or the platform cancelled the touch):
    ///   a drag ends, with `dragend`; the rest of the gesture is the
    ///   terminal's.
    /// - `LongPress`: the terminal took the touch for one before it
    ///   dragged. It never drags, nor taps.
    ///
    /// A touch that drags is a press when it does (SPEC §9.1), at the cell
    /// where it began: `press`, `dragstart`, then what the press causes,
    /// and a `drag` at once if the finger is already over another element.
    /// A touch hovers nothing (§9.4).
    pub fn touch(
        &mut self,
        surface: &str,
        phase: TouchPhase,
        x: f32,
        y: f32,
        mods: Mods,
    ) -> TouchOutcome {
        enter(&mut self.working, surface);
        let mut out = TouchOutcome::default();
        match phase {
            TouchPhase::Down => {
                // A touch that never lifted: its drag ends.
                out.effects = self.cancel_touch();
                let pans = if mods.alt {
                    None
                } else {
                    self.touch_pans(surface, x, y)
                };
                out.touch = if pans.is_some() {
                    Touch::Undecided
                } else {
                    Touch::Terminal
                };
                self.finger = Some(Finger {
                    surface: surface.to_string(),
                    start: (x, y),
                    mods,
                    pans: pans.unwrap_or((true, true)),
                    touch: out.touch,
                    tap: true,
                });
            }
            TouchPhase::Move => {
                let scale = self.config.metrics.scale.max(f32::EPSILON);
                let Some(f) = self.finger.as_mut() else {
                    return out;
                };
                let (dx, dy) = ((x - f.start.0) / scale, (y - f.start.1) / scale);
                if f.tap && dx.hypot(dy) >= TAP_SLOP {
                    f.tap = false;
                }
                match f.touch {
                    Touch::Terminal => {}
                    Touch::Surface => {
                        let s = f.surface.clone();
                        out.touch = Touch::Surface;
                        out.effects = self.pointer_with(&s, PointerKind::Move, x, y, mods, false);
                    }
                    Touch::Undecided if f.tap => out.touch = Touch::Undecided,
                    Touch::Undecided => {
                        // Along the larger delta; a tie pans (SPEC §9.1).
                        let pan = if dx.abs() > dy.abs() {
                            f.pans.0
                        } else if dy.abs() > dx.abs() {
                            f.pans.1
                        } else {
                            true
                        };
                        if pan {
                            f.touch = Touch::Terminal;
                        } else {
                            f.touch = Touch::Surface;
                            let (s, start, keys) = (f.surface.clone(), f.start, f.mods);
                            out.touch = Touch::Surface;
                            out.effects = self.touch_drag(&s, start, keys, (x, y), mods);
                        }
                    }
                }
            }
            TouchPhase::Up => {
                let Some(f) = self.finger.take() else {
                    return out;
                };
                if f.touch == Touch::Surface {
                    out.touch = Touch::Surface;
                    out.effects = self.pointer_with(&f.surface, PointerKind::Up, x, y, mods, false);
                } else if f.tap && self.surfaces.get(&f.surface).is_some_and(|s| s.placed) {
                    out.touch = Touch::Surface;
                    out.effects = self.tap(&f.surface, x, y, f.mods);
                }
            }
            TouchPhase::Cancel => {
                out.effects = self.cancel_touch();
                if let Some(f) = self.finger.as_mut() {
                    f.tap = false;
                }
            }
            TouchPhase::LongPress => {
                if let Some(f) = self.finger.as_mut() {
                    if f.touch == Touch::Surface {
                        out.touch = Touch::Surface;
                    } else {
                        f.touch = Touch::Terminal;
                        f.tap = false;
                    }
                }
            }
        }
        out
    }

    /// A tap at device pixel (`x`, `y`) of `surface`, where the finger
    /// lifted: a click there, which presses (SPEC §9.1: a tap presses at its
    /// lift) but never drags, whatever the keys held (§9.2), and hovers
    /// nothing (§9.4).
    fn tap(&mut self, surface: &str, x: f32, y: f32, mods: Mods) -> Vec<Effect> {
        let mods = Mods { alt: false, ..mods };
        if let Some(s) = self.surfaces.get_mut(surface) {
            s.tapping = true;
        }
        let mut effects = Vec::new();
        for kind in [PointerKind::Move, PointerKind::Down, PointerKind::Up] {
            effects.extend(self.pointer_with(surface, kind, x, y, mods, false));
        }
        if let Some(s) = self.surfaces.get_mut(surface) {
            s.tapping = false;
        }
        effects
    }

    /// The pans a touch at device pixel (`x`, `y`) of `surface` leaves the
    /// touched element, when it may drag there (Surface::touch_pans).
    fn touch_pans(&mut self, surface: &str, x: f32, y: f32) -> Option<(bool, bool)> {
        let m = self.config.metrics;
        let s = self.surfaces.get_mut(surface).filter(|s| s.placed)?;
        let cell = (
            (x / m.cell_w.max(1) as f32).floor() as i32,
            (y / m.cell_h.max(1) as f32).floor() as i32,
        );
        s.touch_pans(x / m.scale, y / m.scale, cell)
    }

    /// A touch becomes a drag (SPEC §9.1): the press it is, at device
    /// pixel `start` where the finger touched with `keys` held, then the
    /// move to `now`, which is a `drag` if the finger is already over
    /// another element.
    fn touch_drag(
        &mut self,
        surface: &str,
        start: (f32, f32),
        keys: Mods,
        now: (f32, f32),
        mods: Mods,
    ) -> Vec<Effect> {
        let scale = self.config.metrics.scale.max(f32::EPSILON);
        // The document learns where the finger touched, as a pointer's
        // move would tell it; the program hears nothing of it.
        if let Some(s) = self.surfaces.get_mut(surface) {
            s.pointer(PointerKind::Move, start.0 / scale, start.1 / scale, keys);
        }
        let mut effects =
            self.pointer_with(surface, PointerKind::Down, start.0, start.1, keys, false);
        effects.extend(self.pointer_with(surface, PointerKind::Move, now.0, now.1, mods, false));
        effects
    }

    /// The touch under way is the terminal's from now: if it drags, its
    /// drag ends, with `dragend` and no target (SPEC §9.1).
    fn cancel_touch(&mut self) -> Vec<Effect> {
        let Some(f) = self.finger.as_mut() else {
            return Vec::new();
        };
        let dragged = f.touch == Touch::Surface;
        f.touch = Touch::Terminal;
        let name = f.surface.clone();
        enter(&mut self.working, &name);
        match self.surfaces.get_mut(&name) {
            Some(s) if dragged => s
                .touch_cancel()
                .into_iter()
                .map(|e| Effect::Reply(e.encode(&name)))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// A wheel's turn, a touchpad's scroll or a touch drag over `surface`,
    /// at device pixel (`x`, `y`) of it, by (`dx`, `dy`) device pixels:
    /// positive scrolls towards the content's end, right and down (a
    /// wheel's notch as far as it scrolls the cells). `surface` is empty
    /// over the cells.
    ///
    /// `taken` is true if the surface took it: its document scrolled, or
    /// `overscroll-behavior` stopped it there (SPEC §5.3). Otherwise the
    /// host handles it as over the cells beneath (§9): scrollback, or the
    /// program's wheel input.
    ///
    /// A gesture goes where its first wheel went: the box that scrolls
    /// takes all of it, and it goes no further when that box reaches its
    /// end, as in a browser. Wheels within 150 ms of each other are one
    /// gesture, and a press or the pointer leaving ends one; a host that
    /// knows when a gesture ends (a touchpad's phases) says so with
    /// [`Host::end_gesture`]. Shift turns a wheel's vertical delta
    /// horizontal, as browsers do.
    pub fn wheel(
        &mut self,
        surface: &str,
        x: f32,
        y: f32,
        dx: f32,
        dy: f32,
        mods: Mods,
    ) -> WheelOutcome {
        enter(&mut self.working, surface);
        let (dx, dy) = if mods.shift && dx == 0.0 {
            (dy, 0.0)
        } else {
            (dx, dy)
        };
        let now = std::time::Instant::now();
        let scale = self.config.metrics.scale;
        let (cx, cy, dx, dy) = (
            x / scale,
            y / scale,
            (dx / scale) as f64,
            (dy / scale) as f64,
        );
        let same = self.gesture.as_ref().filter(|g| {
            now.duration_since(g.at) < GESTURE
                && (g.surface == surface || g.route == scroll::Route::Terminal)
        });
        let route = match same {
            Some(g) => g.route,
            None => match self.surfaces.get_mut(surface).filter(|s| s.placed) {
                Some(s) => s.wheel_route(cx, cy, dx, dy),
                None => scroll::Route::Terminal,
            },
        };
        self.gesture = Some(Gesture {
            surface: surface.to_string(),
            route,
            at: now,
        });
        let mut out = WheelOutcome::default();
        match route {
            scroll::Route::Terminal => {}
            scroll::Route::Stop => out.taken = true,
            scroll::Route::Doc(scroller) => {
                out.taken = true;
                let m = self.config.metrics;
                let passthrough = self.passthrough;
                if let Some(s) = self.surfaces.get_mut(surface) {
                    s.wheel_scroll(scroller, cx, cy, dx, dy);
                    // What the pointer is over changed under it (SPEC §9.4).
                    let cell = (
                        (x / m.cell_w.max(1) as f32).floor() as i32,
                        (y / m.cell_h.max(1) as f32).floor() as i32,
                    );
                    if !s.held() {
                        out.effects.extend(
                            s.hovered(cell, passthrough)
                                .map(|e| Effect::Reply(e.encode(surface))),
                        );
                    }
                }
            }
        }
        out
    }

    /// The wheel gesture under way ended: the next wheel begins another.
    pub fn end_gesture(&mut self) {
        self.gesture = None;
    }

    /// A key for the focused element of `surface`. `consumed` is false when
    /// the element has no use for it; the host then forwards it to the program.
    pub fn key(&mut self, surface: &str, key: &Key) -> KeyOutcome {
        enter(&mut self.working, surface);
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

    /// A key for `surface` as the user pressed it, named as SPEC §10.4
    /// writes it (`Meta+ArrowLeft`), offered before the terminal's own
    /// shortcuts and translations. `consumed` is false, with no effects,
    /// when the surface has no use for it or the name does not parse: the
    /// terminal then goes on, and offers the key again as bytes
    /// ([`Host::key_bytes`]).
    pub fn key_pressed(&mut self, surface: &str, name: &str) -> KeyOutcome {
        match hotty_wire::keys::parse_key(name)
            .as_deref()
            .and_then(Key::from_spec_name)
        {
            Some(key) => self.key(surface, &key),
            None => KeyOutcome::default(),
        }
    }

    /// Keys for the focused element of `surface`, as the terminal would
    /// send them to the program (SPEC §10.4: after its bindings, in the
    /// encoding the program asked for). Each is named from its bytes. When
    /// the surface used none, `consumed` is false and the host sends the
    /// bytes itself; when it used some, the others reach the program as
    /// replies, as they came and in order with the events.
    pub fn key_bytes(&mut self, surface: &str, bytes: &[u8]) -> KeyOutcome {
        enter(&mut self.working, surface);
        enum Out {
            Effect(Effect),
            Pass(std::ops::Range<usize>),
        }
        let mut out = Vec::new();
        let mut consumed = false;
        for (span, name) in hotty_wire::keys::decode_key_spans(bytes) {
            let used = match name.as_deref().and_then(Key::from_spec_name) {
                Some(key) => {
                    let o = self.key(surface, &key);
                    out.extend(o.effects.into_iter().map(Out::Effect));
                    o.consumed
                }
                None => false,
            };
            consumed |= used;
            if !used {
                out.push(Out::Pass(span));
            }
        }
        let effects = out
            .into_iter()
            .filter_map(|o| match o {
                Out::Effect(e) => Some(e),
                Out::Pass(span) if consumed => Some(Effect::Reply(bytes[span].to_vec())),
                Out::Pass(_) => None,
            })
            .collect();
        KeyOutcome { consumed, effects }
    }

    /// Takes the keyboard away from `surface` (the user clicked elsewhere).
    pub fn blur(&mut self, surface: &str) -> Vec<Effect> {
        enter(&mut self.working, surface);
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
        enter(&mut self.working, surface);
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

/// A cell's size in CSS pixels.
fn css_cell(m: &Metrics) -> (f32, f32) {
    let scale = if m.scale > 0.0 { m.scale } else { 1.0 };
    (m.cell_w as f32 / scale, m.cell_h as f32 / scale)
}

/// Whether a new config is the terminal's font changing a cell's size in
/// CSS pixels, which owes each placed surface a `resize` (SPEC §5.3). A
/// zoom keeps the font's size in CSS pixels, and moves a cell's only by
/// the rounding of device pixels, so it owes none. A font derived from the
/// cell (`font_size: None`) changes with it.
fn font_resizes(old: &Config, new: &Config) -> bool {
    let (ow, oh) = css_cell(&old.metrics);
    let (nw, nh) = css_cell(&new.metrics);
    let cell = (ow - nw).abs() > 1e-3 || (oh - nh).abs() > 1e-3;
    let font = old.font_family != new.font_family
        || old.font_size != new.font_size
        || new.font_size.is_none();
    cell && font
}

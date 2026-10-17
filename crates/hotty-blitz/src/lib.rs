//! hotty-blitz: everything a HOTTY host needs between "a command arrived" and
//! "here are the pixels for this cell rectangle".
//!
//! Two hosts use it: the `hotty run` polyfill (through the Rust API)
//! and the Ghostty fork (PoC-2, through [`ffi`]). Neither knows anything about
//! HTML; both hand commands to a [`Host`] and draw the frames it produces.

mod anim;
pub mod body;
pub mod delta;
mod edit;
pub mod fetch;
pub mod ffi;
pub mod input;
pub mod net;
pub mod paint;
pub mod policy;
mod menu;
mod scroll;
pub mod style;
mod surface;

use blitz_dom::FontContext;
use body::Detail;
use hotty_wire::keys::{self, Platform};
use hotty_wire::{Command, Control};
use std::collections::BTreeMap;
use std::sync::Arc;

pub use input::{
    Event, Key, KeyName, KeyOutcome, Mods, PointerKind, Touch, TouchOutcome, TouchPhase,
    WheelOutcome,
};
pub use menu::{MENU, Shown};
pub use surface::{Damage, Frame, Timings};

/// The HOTTY version this host speaks, its only one (SPEC §4): a query
/// that does not list it is answered with `EVERSION`.
pub const VERSION: &str = "0.4";

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

/// How the host reads keys (SPEC §10.2, §10.4–§10.6), as the terminal
/// embedding it sets them ([`Host::set_keys`]). The capabilities report
/// them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeySettings {
    /// What `Mod` stands for, and the edit shortcuts.
    pub platform: Platform,
    /// The terminal takes edit commands from a program in the cells (SPEC
    /// §10.6): the capabilities report `edit`, and the host reads `a=edit`
    /// ([`Effect::Edit`]).
    pub commands: bool,
    /// The terminal shows a program's menu ([`Effect::Menu`], `a=menu`,
    /// SPEC §10.7), which the capabilities then report as the feature
    /// `menu`.
    pub menus: bool,
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
    /// Put the text on the clipboard: a text field's `copy` or `cut` (SPEC
    /// §10.2).
    Clipboard(String),
    /// A text field's `paste` (SPEC §10.2): read the clipboard, and give
    /// its text to [`Host::paste`]. Only where `set_paste` said the
    /// terminal reads it.
    Paste,
    /// `a=edit` (SPEC §10.6): the edit commands the program takes from now
    /// on the screen it is on, in place of what it took before; none, to
    /// take none. The terminal keeps them per screen, sends a key of one
    /// as [`Host::command_event`], and drops them on leaving the alternate
    /// screen and on a full reset.
    Edit(Vec<String>),
    /// Show the level of the program's menu that the keys typed reach, the
    /// surface [`MENU`], `cols`×`rows` cells, at the bottom right of the
    /// terminal's viewport, above everything (SPEC §10.7). Its pixels come
    /// from [`Host::render_dirty`]; [`Effect::Delete`] hides it.
    Menu { surface: String, cols: u16, rows: u16 },
}

/// What [`Host::copy_key`] makes of a key.
#[derive(Debug, PartialEq, Eq)]
pub enum CopyKey {
    /// No surface has a selection outside a text field: the key goes on.
    None,
    /// The selection's keymap binds the key to `copy`: put the text on the
    /// clipboard, and the key goes no further.
    Copy(String),
    /// It does not: the key goes on as if nothing were selected, and the
    /// terminal copies nothing with it.
    Other,
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
    /// The embedding terminal hands over its platform's count of each press
    /// (`pointer_counted`), and the capabilities say `count` (SPEC §9).
    counts: bool,
    /// The embedding terminal reads the clipboard for a field's `paste`
    /// ([`Effect::Paste`], `set_paste`).
    paste: bool,
    /// The run of clicks the last click by the pointer was in: its surface,
    /// the element that reported it (its id, and a link's href), and the
    /// platform's count the run began at.
    click_run: Option<(String, String, Option<String>, u32)>,
    /// `fit` events renders found (SPEC §5.2), at most one per surface:
    /// the rows of the last frame drawn.
    fits: Vec<(String, u16)>,
    /// `view` events renders found (SPEC §9.5), at most one per surface:
    /// what the last frame drawn shows.
    views: Vec<(String, body::View)>,
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
    /// How the host reads keys ([`Host::set_keys`]).
    keys: KeySettings,
    /// The capabilities a program last heard since the last reset, when it
    /// asked for them: it hears `caps` when they change (SPEC §4, §9).
    heard: Option<Heard>,
    /// `caps` events that settings changed, for [`Host::take_events`].
    owed: Vec<Effect>,
    /// The program's menu (SPEC §10.7).
    menu: MenuState,
}

/// The capabilities that can change in a session, as a program last heard
/// them or as they are now (SPEC §4): what a `caps` event is the
/// difference of.
#[derive(Clone, Debug, PartialEq)]
struct Heard {
    cellcss: body::CellCss,
    scale: f64,
    scheme: &'static str,
    modifier: &'static str,
    features: Vec<&'static str>,
    net: policy::Policy,
    edit: Option<body::Edit>,
}

/// The program's menu: what `a=menu` sent on each screen, the level shown
/// with its surface, and what showing or hiding it leaves the embedder to
/// do.
#[derive(Default)]
struct MenuState {
    /// The menus of the main screen and the alternate screen (`a=menu`,
    /// SPEC §10.7), kept while the terminal shows menus.
    main: Option<body::Menu>,
    alt: Option<body::Menu>,
    /// The terminal is on the alternate screen.
    on_alt: bool,
    /// The keys of the level shown (`a=menu:show=1`).
    seq: Option<Vec<String>>,
    shown: Option<(menu::Level, surface::Surface)>,
    /// Something happened that may change them: Host::render_dirty looks.
    stale: bool,
    effects: Vec<Effect>,
}

impl MenuState {
    /// The menu of the screen the terminal is on.
    fn current(&self) -> Option<&body::Menu> {
        if self.on_alt { &self.alt } else { &self.main }.as_ref()
    }

    fn current_mut(&mut self) -> &mut Option<body::Menu> {
        if self.on_alt { &mut self.alt } else { &mut self.main }
    }
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

/// Whether a wheel gesture still scrolls the document of `surface` at
/// `now`: the gesture under way, a wheel's notches or a touchpad's scroll
/// and its momentum, went to it and has not come to rest. A `view` waits
/// for its rest (SPEC §9.5).
fn scrolling(gesture: &Option<Gesture>, surface: &str, now: std::time::Instant) -> bool {
    gesture.as_ref().is_some_and(|g| {
        g.surface == surface && matches!(g.route, scroll::Route::Doc(_)) && now < g.at + GESTURE
    })
}

/// When the `view` that `s`, named `surface`, owes (SPEC §9.5) can be found:
/// once the gesture that scrolled it comes to rest, `now` if none moves.
/// None while it owes none, or while a thumb is held: the release draws
/// the surface again.
fn view_rest(
    gesture: &Option<Gesture>,
    surface: &str,
    s: &surface::Surface,
    now: std::time::Instant,
) -> Option<std::time::Instant> {
    if !(s.placed && s.view_due) || s.scroll_held() {
        return None;
    }
    Some(match gesture {
        Some(g) if g.surface == surface && matches!(g.route, scroll::Route::Doc(_)) => {
            (g.at + GESTURE).max(now)
        }
        _ => now,
    })
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
            renderers: surface::Renderers::default(),
            created: 0,
            program_press: false,
            passthrough: false,
            counts: false,
            paste: false,
            click_run: None,
            fits: Vec::new(),
            views: Vec::new(),
            gesture: None,
            finger: None,
            frame_log: FrameLog::open(),
            working: String::new(),
            lost: Vec::new(),
            keys: KeySettings::default(),
            heard: None,
            owed: Vec::new(),
            menu: MenuState::default(),
        }
    }

    /// How the host reads keys, as the terminal embedding it sets them
    /// (SPEC §10.2, §10.4–§10.6). A program that asked for the capabilities
    /// hears the fields that changed as `caps` (SPEC §9): the effects.
    pub fn set_keys(&mut self, keys: KeySettings) -> Vec<Effect> {
        if keys == self.keys {
            return Vec::new();
        }
        self.keys = keys;
        for s in self.surfaces.values_mut() {
            s.settings = self.keys.clone();
        }
        self.menu.stale = true;
        if !self.keys.menus {
            self.menu.main = None;
            self.menu.alt = None;
            self.menu.seq = None;
        }
        self.caps_changed(false)
    }

    /// The capabilities that can change as they are now (SPEC §4).
    fn snapshot(&self) -> Heard {
        let features = [
            ("passthrough", self.passthrough),
            // Every host of this one sends `hover`; the embedder passes the
            // pointer's leaving (`PointerKind::Leave`).
            ("hover", true),
            ("count", self.counts),
            ("menu", self.keys.menus),
        ];
        Heard {
            cellcss: cellcss(&self.config.metrics),
            scale: body::float(self.config.metrics.scale),
            scheme: if self.config.theme.dark { "dark" } else { "light" },
            modifier: self.keys.platform.mod_name(),
            features: features.into_iter().filter(|f| f.1).map(|f| f.0).collect(),
            net: self.store.host_policy(),
            edit: edit_of(&self.keys),
        }
    }

    /// The `caps` event a program that asked for the capabilities is owed
    /// (SPEC §4, §9): the fields that differ from what it last heard, a
    /// field that became absent as its type's empty value. `cell` is true
    /// when the terminal's font changed, which alone changes `cellcss`: a
    /// zoom moves a cell's device pixels and `scale` with it, and `scale`
    /// is only sent along with a change of another field. None when
    /// nothing is owed.
    fn caps_changed(&mut self, cell: bool) -> Vec<Effect> {
        let Some(old) = self.heard.clone() else {
            return Vec::new();
        };
        let now = self.snapshot();
        let mut c = body::Changed {
            cellcss: (cell && old.cellcss != now.cellcss).then_some(now.cellcss),
            scheme: (old.scheme != now.scheme).then_some(now.scheme),
            modifier: (old.modifier != now.modifier).then_some(now.modifier),
            features: (old.features != now.features).then(|| now.features.clone()),
            net: (old.net != now.net).then(|| now.net.clone()),
            edit: (old.edit != now.edit).then(|| now.edit.clone().unwrap_or_default()),
            scale: None,
        };
        if c == body::Changed::default() {
            return Vec::new();
        }
        c.scale = (old.scale != now.scale).then_some(now.scale);
        // What the program heard now: the fields sent, and the rest as they were.
        let mut next = now;
        if c.cellcss.is_none() {
            next.cellcss = old.cellcss;
        }
        if c.scale.is_none() {
            next.scale = old.scale;
        }
        self.heard = Some(next);
        vec![Effect::Reply(terminal_event("caps", &Detail::Caps(c)))]
    }

    /// The edit command a key is (SPEC §10.6): the edit action whose
    /// shortcut it is, named as SPEC §10.4 names it.
    pub fn edit_command(&self, name: &str) -> Option<String> {
        self.keys.platform.edit_action(name)
    }

    /// The event `command` (SPEC §10.6) for the program in the cells: the
    /// command, its key (none from a menu), and a paste's text (none when
    /// the clipboard holds no text).
    pub fn command_event(name: &str, key: Option<&str>, text: Option<&str>) -> Vec<u8> {
        terminal_event(
            "command",
            &Detail::Command {
                name: name.to_string(),
                key: key.map(str::to_string),
                text: text.map(str::to_string),
            },
        )
    }

    /// A key that reaches the program in the cells, named as SPEC §10.4
    /// names it, while it shows a level of its menu (SPEC §10.7): `escape`,
    /// and a key the level does not list, hide it. Keys that reach no group
    /// list none. The key still reaches the program.
    pub fn cells_key(&mut self, name: &str) {
        self.cells_names(&[name.to_string()]);
    }

    /// [`Host::cells_key`] for a press with several names: it is listed if
    /// the level lists either.
    fn cells_names(&mut self, names: &[String]) {
        if self.menu.seq.is_none() {
            return;
        }
        let p = &self.keys.platform;
        let read: Vec<String> = names.iter().filter_map(|n| keys::read_key(p, n)).collect();
        let listed = self.level().is_some_and(|level| {
            level
                .items
                .iter()
                .any(|i| keys::read_key(p, &i.key).is_some_and(|k| read.contains(&k)))
        });
        if read.iter().any(|n| n == "escape") || !listed {
            self.hide_level();
        }
    }

    /// The level shown hides (SPEC §10.7).
    pub fn hide_level(&mut self) {
        if self.menu.seq.take().is_some() {
            self.menu.stale = true;
        }
    }

    /// The level of the current screen's menu that the keys shown reach.
    fn level(&self) -> Option<menu::Level> {
        let seq = self.menu.seq.as_ref()?;
        menu::level(self.menu.current()?, seq, &self.keys.platform)
    }

    /// The items of the level shown (SPEC §10.7), keys and labels as the
    /// program sent them; none when no level shows. The test interface's
    /// (SPEC §16).
    #[doc(hidden)]
    pub fn menu_shown(&self) -> Option<Vec<Shown>> {
        self.level().map(|l| l.items)
    }

    /// The terminal's screen changed, or is told for the first time (SPEC
    /// §10.7): leaving the alternate screen drops its menu, as the edit
    /// commands taken there are; any change hides the level shown.
    pub fn set_screen(&mut self, alt: bool) {
        if alt == self.menu.on_alt {
            return;
        }
        if self.menu.on_alt {
            self.menu.alt = None;
        }
        self.menu.on_alt = alt;
        self.menu.seq = None;
        self.menu.stale = true;
    }

    /// The event `command` (SPEC §10.6, §10.7) of a pick of the item named
    /// `name`: the first with that name, depth-first. None for an item that
    /// is not enabled, a name no item has, or no menu.
    pub fn menu_pick(&self, name: &str) -> Option<Vec<u8>> {
        if name.is_empty() {
            return None;
        }
        let item = menu::find(&self.menu.current()?.items, name)?;
        item.enabled.then(|| Host::command_event(name, None, None))
    }

    /// Sets the current screen's menu; the level shown refreshes with the
    /// keys it has (SPEC §10.7).
    fn set_menu(&mut self, menu: Option<body::Menu>) {
        *self.menu.current_mut() = menu;
        self.menu.stale = true;
    }

    /// Shows the level the keys typed reach, or hides it (SPEC §10.7).
    fn sync_menu(&mut self) {
        if !std::mem::take(&mut self.menu.stale) {
            return;
        }
        let want = if self.keys.menus { self.level() } else { None };
        if want.as_ref() != self.menu.shown.as_ref().map(|(l, _)| l) {
            self.sync_level(want);
        }
    }

    /// A document of the host's own, placed at once for a terminal that
    /// draws it as soon as it has its size.
    fn menu_surface(&self, html: &str) -> surface::Surface {
        surface::Surface::new(
            html,
            &self.config,
            &self.css,
            self.store.clone(),
            self.font_ctx.clone(),
            (500, 100),
            0,
        )
    }

    fn sync_level(&mut self, want: Option<menu::Level>) {
        if let Some((_, old)) = self.menu.shown.take() {
            self.store.forget_document(old.doc_id());
            if want.is_none() {
                self.menu.effects.push(Effect::Delete {
                    surface: MENU.to_string(),
                });
            }
        }
        let Some(l) = want else {
            return;
        };
        // Laid out wide, then cut to the cells what it shows covers.
        let mut s = self.menu_surface(&menu::menu_html(&l, &self.keys.platform));
        let (w, h) = css_cell(&self.config.metrics);
        let (mw, mh) = s.element_size("m").unwrap_or_default();
        let cells = |px: f64, cell: f32| ((px - 0.5) / f64::from(cell.max(1.0))).ceil().clamp(1.0, 500.0) as u16;
        let (cols, rows) = (cells(mw, w), cells(mh, h));
        s.set_size(cols, rows);
        s.placed = true;
        s.dirty = true;
        s.redeliver = true;
        self.menu.effects.push(Effect::Menu {
            surface: MENU.to_string(),
            cols,
            rows,
        });
        self.menu.shown = Some((l, s));
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// New cell size, theme or font: every surface restyles and re-renders at
    /// the new pixel size, with no help from the program (PoC-2's promise).
    /// A zoom (SPEC §5.3) changes `scale` with the cells, so a cell keeps its
    /// size in CSS pixels and the layout stays: only `--hotty-px` (§8)
    /// changes, and the program hears nothing. A new font changes the cell
    /// in CSS pixels: each placed surface is laid out again, and a program
    /// that asked for the capabilities hears `caps` with the new `cellcss`
    /// (SPEC §4, §9), as does one whose terminal's theme changed. The event
    /// goes out with [`Host::take_events`].
    pub fn set_config(&mut self, config: Config) {
        if config == self.config {
            return;
        }
        let font = font_changed(&self.config, &config);
        let css = style::host_css(&config);
        let own = self.menu.shown.as_mut().map(|(_, s)| s);
        for s in self.surfaces.values_mut().chain(own) {
            s.replace_host_css(&self.css, &css);
            s.dirty = true;
        }
        self.css = css;
        self.config = config;
        let owed = self.caps_changed(font);
        self.owed.extend(owed);
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
        let shown = match surface {
            MENU => self.menu.shown.as_mut().map(|(_, s)| s),
            _ => None,
        };
        if let Some(s) = shown {
            s.redeliver = true;
            s.dirty = true;
            return;
        }
        enter(&mut self.working, surface);
        if let Some(s) = self.surfaces.get_mut(surface).filter(|s| s.placed) {
            s.redeliver = true;
            s.dirty = true;
        }
    }

    /// Whether [`Host::render_dirty`] has something to render: a placed
    /// surface changed, an animated image on one is due to show its next
    /// frame, something a document fetched arrived, or a scroll came to
    /// rest that held a `view` back (SPEC §9.5).
    pub fn has_dirty(&self) -> bool {
        let now = std::time::Instant::now();
        // Lost surfaces' deletions go out with the next frame's events, and
        // so do the `caps` that settings changed, and the menu.
        !self.lost.is_empty()
            || !self.owed.is_empty()
            || self.menu.stale
            || self.menu.shown.as_ref().is_some_and(|(_, s)| s.dirty)
            || self.store.has_delivered()
            || self.surfaces.iter().any(|(name, s)| {
                s.placed
                    && (s.dirty
                        || s.next_frame().is_some_and(|t| t <= now)
                        || view_rest(&self.gesture, name, s, now).is_some_and(|t| t <= now))
            })
    }

    /// When an animated image (GIF, APNG, WebP) on a placed surface is due
    /// to show its next frame, scrollbars that fade are drawn again, or a
    /// scroll that held a `view` back comes to rest (SPEC §9.5): render
    /// then. `None` while nothing plays, or nothing that plays is in a
    /// placement's window, no scrollbar fades and no `view` waits. Each
    /// animated image has one deadline at a time, so a host needs one
    /// timer. Now, when something a document fetched has arrived, or
    /// `caps` is owed.
    pub fn next_frame(&self) -> Option<std::time::Instant> {
        let now = std::time::Instant::now();
        if self.store.has_delivered() || !self.owed.is_empty() {
            return Some(now);
        }
        self.surfaces
            .iter()
            .filter(|(_, s)| s.placed)
            .flat_map(|(name, s)| {
                let rest = view_rest(&self.gesture, name, s, now);
                s.next_frame().into_iter().chain(rest)
            })
            .min()
    }

    /// The host's half of the network policy (SPEC §7.2), in CSP's syntax:
    /// `img-src https://example.com; font-src https:`. A host run by a
    /// person starts with none, and its user grants it. The capabilities
    /// report it (`net`), and a program that asked for them hears a change
    /// as `caps` (SPEC §9), with [`Host::take_events`]. Fetching happens on
    /// threads of its own; set a waker ([`Host::set_waker`]) to hear when
    /// something arrives.
    pub fn set_network(&mut self, policy: &str) {
        self.store.set_host_policy(policy::Policy::parse(policy));
        let owed = self.caps_changed(false);
        self.owed.extend(owed);
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
        let mut effects: Vec<Effect> = names
            .into_iter()
            .filter_map(|n| self.remove_surface(&n))
            .collect();
        // The program's menus go, and the capabilities it asked for: the
        // next program asks again (SPEC §10.7).
        self.menu.main = None;
        self.menu.alt = None;
        self.menu.seq = None;
        self.menu.stale = false;
        self.menu.effects.clear();
        self.owed.clear();
        self.heard = None;
        if let Some((_, s)) = self.menu.shown.take() {
            self.store.forget_document(s.doc_id());
            effects.push(Effect::Delete {
                surface: MENU.to_string(),
            });
        }
        effects
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
        self.sync_menu();
        let metrics = self.config.metrics;
        let dark = self.config.theme.dark;
        let now = std::time::Instant::now();
        for (name, s) in self.surfaces.iter_mut() {
            // A scroll came to rest: the `view` it held back (SPEC §9.5).
            if view_rest(&self.gesture, name, s, now).is_some_and(|t| t <= now) {
                s.dirty = true;
            }
            if !(s.dirty && s.placed) {
                continue;
            }
            enter(&mut self.working, name);
            let moving = scrolling(&self.gesture, name, now);
            let damage = s.render(&metrics, dark, moving, &mut self.renderers);
            if let Some(rows) = s.fit_event.take() {
                match self.fits.iter_mut().find(|(n, _)| n == name) {
                    Some(f) => f.1 = rows,
                    None => self.fits.push((name.clone(), rows)),
                }
            }
            if let Some(view) = s.view_event.take() {
                match self.views.iter_mut().find(|(n, _)| n == name) {
                    Some(v) => v.1 = view,
                    None => self.views.push((name.clone(), view)),
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
        if let Some((_, s)) = self.menu.shown.as_mut()
            && s.dirty
        {
            self.working.clear();
            if let Some(damage) = s.render(&metrics, dark, false, &mut self.renderers) {
                out(MENU, &s.frame, &damage);
            }
        }
    }

    /// The events rendering and the terminal's settings produced, for the
    /// program: `caps` (SPEC §4, §9) after the terminal's font, theme or
    /// other settings changed what its capabilities say, `fit` (SPEC §5.2),
    /// with the rows of the last frame drawn, and `view` (§9.5), with what
    /// it shows, at most one each per surface. Take them after
    /// [`Host::render_dirty`].
    pub fn take_events(&mut self) -> Vec<Effect> {
        let lost = std::mem::take(&mut self.lost);
        let owed = std::mem::take(&mut self.owed);
        let fits = std::mem::take(&mut self.fits)
            .into_iter()
            .filter(|(name, _)| {
                self.surfaces
                    .get(name)
                    .is_some_and(|s| s.placed && s.fit.is_some() && !s.is_detached())
            })
            .map(|(name, rows)| {
                let e = Event {
                    kind: "fit",
                    target: String::new(),
                    detail: Some(Detail::Fit { r: rows }),
                };
                Effect::Reply(e.encode(&name))
            });
        let views = std::mem::take(&mut self.views)
            .into_iter()
            .filter(|(name, _)| {
                self.surfaces
                    .get(name)
                    .is_some_and(|s| s.placed && !s.is_detached())
            })
            .map(|(name, view)| {
                let e = Event {
                    kind: "view",
                    target: String::new(),
                    detail: Some(Detail::View(view)),
                };
                Effect::Reply(e.encode(&name))
            });
        let menu = std::mem::take(&mut self.menu.effects);
        lost.into_iter()
            .chain(menu)
            .chain(owed)
            .chain(fits)
            .chain(views)
            .collect()
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
        self.menu.stale = true;
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
        let mut failed = None;
        let result = self.dispatch(cmd, &mut effects, &mut failed);
        // Without `q`, a command is answered only with an error, unless it
        // has `n` or is a query (SPEC §3.5, §3.6).
        let quiet: u8 = cmd
            .get("q")
            .and_then(|q| q.parse().ok())
            .filter(|&q| q <= 2)
            .unwrap_or(if cmd.action() == "q" || cmd.get("n").is_some() { 0 } else { 1 });
        match result {
            Ok(extra) => {
                if quiet == 0 && cmd.action() != "ev" {
                    effects.push(Effect::Reply(reply("ok", cmd, extra.0, extra.1)));
                }
            }
            Err((code, detail)) => {
                if quiet < 2 {
                    let err = body::encode(&body::Error {
                        code,
                        detail: &detail,
                    });
                    // A delta's error names the op that failed (SPEC §3.6).
                    let at = failed.map(|i| ("i".to_string(), i.to_string()));
                    effects.push(Effect::Reply(reply("err", cmd, at.into_iter().collect(), Some(err))));
                }
            }
        }
        effects
    }

    /// Does what a command asks. `failed` is the index of the op of a delta
    /// that an error names (SPEC §6.1).
    fn dispatch(
        &mut self,
        cmd: &Command,
        effects: &mut Vec<Effect>,
        failed: &mut Option<usize>,
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
                // `v` lists the versions the program speaks, comma-separated
                // (SPEC §4), and this host answers in the highest it shares:
                // 0.4 when an entry is exactly that, whatever the others and
                // their order. Otherwise, a query with no `v` too, EVERSION,
                // its detail the versions this host speaks.
                if !cmd
                    .get("v")
                    .is_some_and(|v| v.split(',').any(|v| v == VERSION))
                {
                    return Err(("EVERSION", VERSION.to_string()));
                }
                let now = self.snapshot();
                self.heard = Some(now.clone());
                let caps = body::Caps {
                    v: VERSION,
                    cellcss: now.cellcss,
                    scale: now.scale,
                    scheme: now.scheme,
                    modifier: now.modifier,
                    features: now.features,
                    limits: body::Limits {
                        resources: self.store.quota as u64,
                        message: hotty_wire::MAX_COMMAND_BYTES as u64,
                    },
                    // The host's half of the network policy (SPEC §7.2):
                    // empty unless its user granted something.
                    net: now.net,
                    edit: now.edit,
                    host: "hotty-blitz",
                    // Raised with a change a program may need to know of
                    // (SPEC §15): 0.0.2 paints a box resized through a var
                    // delta (Blitz fork 0005); 0.0.3 takes `a=delta`, which
                    // was `a=patch`, and no longer the old name.
                    version: env!("CARGO_PKG_VERSION"),
                };
                Ok((Vec::new(), Some(body::encode(&caps))))
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
                s.settings = self.keys.clone();
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
                // What the program hears belongs to the surface: `1` turns it
                // on, `0` off, and a placement without it, or with any other
                // value, leaves it as it was (SPEC §5.2).
                match cmd.get("p") {
                    Some("1") => s.presses = true,
                    Some("0") => s.presses = false,
                    _ => {}
                }
                // `fit` starts from the placement's own rows.
                match cmd.get("f") {
                    Some("1") => s.fit = Some(rows),
                    Some("0") => s.fit = None,
                    _ if s.fit.is_some() => s.fit = Some(rows),
                    _ => {}
                }
                // `hover` starts from out when it is turned on (SPEC §9.4).
                match cmd.get("v") {
                    Some("1") => {
                        s.hover.get_or_insert(surface::Hovered::Out);
                    }
                    Some("0") => s.hover = None,
                    _ => {}
                }
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
                // The whole delta is checked before any op applies (SPEC
                // §6.1).
                let ops = body::decode_delta(&cmd.payload).map_err(|f| {
                    *failed = f.index;
                    ("EINVAL", f.detail)
                })?;
                s.apply(&ops, &self.config.metrics).map_err(|(i, e)| {
                    *failed = Some(i);
                    (e.code, e.detail)
                })?;
                Ok((Vec::new(), None))
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
                // What the program hears stays asked for (SPEC §5.2), and
                // goes on from what it last heard once the surface is placed
                // again (§9).
                if s.placed {
                    s.placed = false;
                    s.window = None;
                    s.hidden();
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
            // The edit commands the program takes (SPEC §10.6), where the
            // terminal takes them: the names it knows, in §10.2's order.
            "edit" if self.keys.commands => {
                let take = cmd.get("take").unwrap_or_default();
                let names = keys::EDIT_ACTIONS
                    .iter()
                    .filter(|a| take.split(',').any(|t| t == **a))
                    .map(|a| a.to_string())
                    .collect();
                effects.push(Effect::Edit(names));
                Ok((Vec::new(), None))
            }
            // A program's menu (SPEC §10.7): no body leaves it none. With
            // `show`, the level its keys reach shows, or hides.
            "menu" if self.keys.menus => {
                match cmd.get("show") {
                    None => {
                        let m = if cmd.payload.is_empty() {
                            None
                        } else {
                            Some(body::decode_menu(&cmd.payload).map_err(|e| ("EINVAL", format!("bad menu: {e}")))?)
                        };
                        self.set_menu(m);
                    }
                    Some("1") => {
                        self.menu.seq = Some(
                            body::decode_seq(&cmd.payload).map_err(|e| ("EINVAL", format!("bad seq: {e}")))?,
                        );
                    }
                    Some("0") => self.menu.seq = None,
                    Some(other) => return Err(("EINVAL", format!("show={other} is not 0 or 1"))),
                }
                self.menu.stale = true;
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
        self.pointer_with(surface, kind, x, y, mods, true, 0)
    }

    /// [`Host::pointer`], with the platform's count of a press and its
    /// release in a run of clicks (macOS's clickCount, GTK's n_press): the
    /// clicks a release makes carry their place in their run (SPEC §9),
    /// which starts again on another element or surface. 0 is a count the
    /// embedder does not know, and reports none.
    pub fn pointer_counted(
        &mut self,
        surface: &str,
        kind: PointerKind,
        x: f32,
        y: f32,
        mods: Mods,
        count: u32,
    ) -> Vec<Effect> {
        self.pointer_with(surface, kind, x, y, mods, true, count)
    }

    /// [`Host::pointer`], reporting `hover` only when `hover` is true: a
    /// touch's drag hovers nothing (SPEC §9.4).
    #[allow(clippy::too_many_arguments)]
    fn pointer_with(
        &mut self,
        surface: &str,
        kind: PointerKind,
        x: f32,
        y: f32,
        mods: Mods,
        hover: bool,
        count: u32,
    ) -> Vec<Effect> {
        enter(&mut self.working, surface);
        // A press begins a gesture, and decides whose it is: a release the
        // host lost does not leave the next one the program's.
        if kind == PointerKind::Down {
            self.program_press = mods.alt;
            // One selection across the host (SPEC §11): a press anywhere
            // else drops it, and a press with Alt is on the cells.
            let keep = (!mods.alt).then_some(surface);
            self.clear_selection(keep);
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
        let (lead, mut events) = s.pointer_input(kind, x / m.scale, y / m.scale, cell, mods);
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
                s.pressed(mods).map(|e| Effect::Reply(e.encode(surface)))
            }
            _ => None,
        }
        .into_iter()
        .chain(lead.into_iter().map(|e| Effect::Reply(e.encode(surface))))
        .collect();
        if kind == PointerKind::Up {
            count_clicks(&mut self.click_run, surface, &mut events, count);
        }
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
        // A tap is a click of its own (SPEC §9).
        self.click_run = None;
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
                        out.effects = self.pointer_with(&s, PointerKind::Move, x, y, mods, false, 0);
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
                    out.effects = self.pointer_with(&f.surface, PointerKind::Up, x, y, mods, false, 0);
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
            effects.extend(self.pointer_with(surface, kind, x, y, mods, false, 0));
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
            self.pointer_with(surface, PointerKind::Down, start.0, start.1, keys, false, 0);
        effects.extend(self.pointer_with(surface, PointerKind::Move, now.0, now.1, mods, false, 0));
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
    /// horizontal, as browsers do. The `view` a gesture changes (SPEC
    /// §9.5) waits for it to come to rest: [`Host::next_frame`] says when.
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
    /// After a prefix (SPEC §10.5) none is used.
    pub fn key(&mut self, surface: &str, key: &Key) -> KeyOutcome {
        self.offer(surface, key, surface::Offer::Typed)
    }

    fn offer(&mut self, surface: &str, key: &Key, offer: surface::Offer) -> KeyOutcome {
        enter(&mut self.working, surface);
        // A key ends a run of clicks: a click it makes is a first (SPEC §9).
        self.click_run = None;
        self.menu.stale = true;
        // A key the level shown does not list hides it (SPEC §10.7).
        let names = key.names();
        if names.is_empty() {
            self.hide_level();
        } else {
            self.cells_names(&names);
        }
        let Some(s) = self.surfaces.get_mut(surface) else {
            return KeyOutcome::default();
        };
        s.can_paste = self.paste;
        let (consumed, events) = s.key(key, offer);
        let mut effects: Vec<Effect> = events
            .into_iter()
            .map(|e| Effect::Reply(e.encode(surface)))
            .collect();
        effects.extend(s.clipboard.take().map(Effect::Clipboard));
        if std::mem::take(&mut s.paste_wanted) {
            effects.push(Effect::Paste);
        }
        KeyOutcome { consumed, effects }
    }

    /// Whether the embedding terminal reads the clipboard for a text
    /// field's `paste` ([`Effect::Paste`]). Without it, the key bound to
    /// `paste` reaches the program (SPEC §10.2).
    pub fn set_paste(&mut self, reads: bool) {
        self.paste = reads;
    }

    /// Types `text` into the focused text field of the surface that has the
    /// keyboard, as a field's `paste` does (SPEC §10.2, §11): the clipboard
    /// read for an [`Effect::Paste`], or the terminal's own paste. `None`
    /// when no text field has the keyboard: the paste is the program's, as
    /// usual.
    pub fn paste(&mut self, text: &str) -> Option<Vec<Effect>> {
        let surface = self.focused_surface()?.to_string();
        enter(&mut self.working, &surface);
        let events = self.surfaces.get_mut(&surface)?.paste(text)?;
        Some(
            events
                .into_iter()
                .map(|e| Effect::Reply(e.encode(&surface)))
                .collect(),
        )
    }

    /// A key for `surface` as the user pressed it, offered before the
    /// terminal's own shortcuts and translations, with the names SPEC §10.4
    /// gives a press: the physical key (`ctrl+key_w`) and the other, the
    /// character it types (`ctrl+w`) or, for a modifier tapped alone, its
    /// modifier key (`shift`), each possibly empty. The surface uses the
    /// press if it uses either name, and where its keymap binds both, the
    /// physical binding wins. `consumed` is false, with no effects, when the
    /// surface has no use for it or neither name parses: the terminal then
    /// goes on, and offers the key again as bytes ([`Host::key_bytes`]).
    pub fn key_pressed(&mut self, surface: &str, physical: &str, other: &str) -> KeyOutcome {
        match Key::pressed(physical, other) {
            Some(key) => self.offer(surface, &key, surface::Offer::Pressed),
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
        self.menu.stale = true;
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
    /// a surface does not take it (SPEC §9.3): the capabilities list the
    /// feature `passthrough`, and a program that asked for them hears a
    /// change as `caps`, with [`Host::take_events`].
    pub fn set_passthrough(&mut self, on: bool) {
        self.passthrough = on;
        let owed = self.caps_changed(false);
        self.owed.extend(owed);
    }

    /// The terminal embedding this host hands over its platform's count of
    /// each press (`pointer_counted`): the capabilities list the feature
    /// `count` (SPEC §4, §9), and a program that asked for them hears a
    /// change as `caps`, with [`Host::take_events`].
    pub fn set_counts(&mut self, on: bool) {
        self.counts = on;
        let owed = self.caps_changed(false);
        self.owed.extend(owed);
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

    /// The text selected in the host, for its copy key (SPEC §11): in the
    /// surface holding the keyboard first, else in any surface. A press
    /// leaves at most one surface with a selection.
    pub fn selected_text(&self) -> Option<String> {
        let focused = self.focused_surface().and_then(|n| self.surfaces.get(n));
        focused
            .and_then(surface::Surface::selected_text)
            .or_else(|| self.surfaces.values().find_map(surface::Surface::selected_text))
    }

    /// A key, named as SPEC §10.4 writes it, for the selection in a
    /// surface's document outside a text field (SPEC §11), which the
    /// terminal offers every key first, before the surface that has the
    /// keyboard and its own shortcuts, whether or not a surface has the
    /// keyboard. A focused text field's selection is its keymap's, through
    /// [`Host::key`].
    pub fn copy_key(&self, name: &str) -> CopyKey {
        let Some(name) = hotty_wire::keys::parse_key(name) else {
            return CopyKey::None;
        };
        let focused = self.focused_surface().and_then(|n| self.surfaces.get(n));
        let found = focused
            .and_then(|s| s.copy_key(&name))
            .or_else(|| self.surfaces.values().find_map(|s| s.copy_key(&name)));
        match found {
            None => CopyKey::None,
            Some(Some(text)) => CopyKey::Copy(text),
            Some(None) => CopyKey::Other,
        }
    }

    /// Drops the selection of every surface but `keep`: the user pressed
    /// elsewhere, or selected on the cells (SPEC §11).
    pub fn clear_selection(&mut self, keep: Option<&str>) {
        for (name, s) in self.surfaces.iter_mut() {
            if Some(name.as_str()) != keep {
                s.clear_selection();
            }
        }
    }

    pub fn focused_surface(&self) -> Option<&str> {
        self.surfaces
            .iter()
            .find(|(_, s)| s.has_focus())
            .map(|(n, _)| n.as_str())
    }
}

/// An event of the terminal's, of no surface (SPEC §9): `command` and
/// `caps`, with neither `s` nor `t`.
fn terminal_event(kind: &str, detail: &Detail) -> Vec<u8> {
    let mut c = Control::default();
    c.set("a", "ev");
    c.set("e", kind);
    hotty_wire::encode_plain(&c, &body::encode(detail))
}

/// The edit shortcuts the capabilities report (SPEC §4): only where the
/// terminal takes edit commands.
fn edit_of(k: &KeySettings) -> Option<body::Edit> {
    k.commands.then(|| k.platform.edit().into_iter().collect())
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

/// A cell's size in CSS pixels as the capabilities report it (SPEC §4), to
/// a thousandth: no f32 noise on the wire.
fn cellcss(m: &Metrics) -> body::CellCss {
    let scale = if m.scale > 0.0 { body::float(m.scale) } else { 1.0 };
    let px = |device: u32| (f64::from(device) / scale * 1000.0).round() / 1000.0;
    body::CellCss {
        w: px(m.cell_w),
        h: px(m.cell_h),
    }
}

/// Whether a new config is the terminal's font changing a cell's size in
/// CSS pixels, which `caps` tells (SPEC §4, §5.3). A zoom keeps the font's
/// size in CSS pixels, and moves a cell's only by the rounding of device
/// pixels, so it tells nothing. A font derived from the cell
/// (`font_size: None`) changes with it.
fn font_changed(old: &Config, new: &Config) -> bool {
    let (ow, oh) = css_cell(&old.metrics);
    let (nw, nh) = css_cell(&new.metrics);
    let cell = (ow - nw).abs() > 1e-3 || (oh - nh).abs() > 1e-3;
    let font = old.font_family != new.font_family
        || old.font_size != new.font_size
        || new.font_size.is_none();
    cell && font
}

/// Numbers the clicks a release made (SPEC §9): `count` is the platform's
/// count of the press, and a run restarts where the element or the surface
/// changes. A click from 2 on carries its place; 0, a count the embedder
/// does not know, numbers none and ends the run.
fn count_clicks(
    run: &mut Option<(String, String, Option<String>, u32)>,
    surface: &str,
    events: &mut [input::Event],
    count: u32,
) {
    for e in events.iter_mut().filter(|e| e.kind == "click") {
        if count == 0 {
            *run = None;
            continue;
        }
        let href = match &e.detail {
            Some(body::Detail::Click { href, .. }) => href.clone(),
            _ => None,
        };
        let base = match run {
            Some((s, t, h, base))
                if count > *base && s == surface && *t == e.target && *h == href =>
            {
                *base
            }
            _ => count,
        };
        *run = Some((surface.to_string(), e.target.clone(), href, base));
        let n = count - base + 1;
        if n < 2 {
            continue;
        }
        match &mut e.detail {
            Some(body::Detail::Click { count, .. }) => *count = Some(n),
            d @ None => {
                *d = Some(body::Detail::Click {
                    area: None,
                    mods: None,
                    href: None,
                    url: None,
                    value: None,
                    count: Some(n),
                })
            }
            _ => {}
        }
    }
}

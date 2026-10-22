//! C ABI for hosts that are not Rust (the Ghostty fork, PoC-2; see
//! `include/hotty_blitz.h`). A host owns one `hotty_host` per terminal, calls it
//! from one thread at a time, and hands it OSC 7279 bodies as its parser
//! finishes them. Everything the host must do comes back through callbacks.

use crate::{
    CellsTouch, Config, CopyKey, Damage, Effect, Host, Metrics, Mods, PointerKind, Rgb, Theme, Touch,
    TouchPhase,
};
use hotty::{Decoder, Fed};
use std::ffi::{CStr, CString, c_char, c_int, c_void};

#[repr(C)]
pub struct HottyConfig {
    pub cell_w: u32,
    pub cell_h: u32,
    pub scale: f32,
    /// 0xRRGGBB
    pub fg: u32,
    pub bg: u32,
    pub palette: [u32; 16],
    pub dark: bool,
    /// UTF-8, may be null.
    pub font_family: *const c_char,
    /// The terminal's font size in CSS px; 0 derives it from `cell_h`.
    pub font_size: f32,
}

#[repr(C)]
pub struct HottyEffects {
    pub ctx: *mut c_void,
    /// Bytes for the program's input.
    pub reply: Option<extern "C" fn(ctx: *mut c_void, data: *const u8, len: usize)>,
    /// Place `surface` at the cursor: it is `cols`×`rows` cells, and the
    /// placement shows the window `x`, `y`, `w`×`h` of it, over `w`×`h`
    /// cells, above or below overlapping placements by `z` (SPEC §5.2:
    /// greater above; among equals, the one whose surface was `created`
    /// later above). An adapter without the surface's pixels asks for them
    /// with `hotty_host_redeliver`.
    pub place: Option<
        extern "C" fn(
            ctx: *mut c_void,
            surface: *const c_char,
            cols: u16,
            rows: u16,
            x: u16,
            y: u16,
            w: u16,
            h: u16,
            z: i32,
            created: u32,
            move_cursor: bool,
        ),
    >,
    /// Remove `surface`'s placement: the surface is gone.
    pub remove: Option<extern "C" fn(ctx: *mut c_void, surface: *const c_char)>,
    /// Remove `surface`'s placement and keep what shows it again cheaply: the
    /// surface is hidden (SPEC §5.4) and its document stays.
    pub hide: Option<extern "C" fn(ctx: *mut c_void, surface: *const c_char)>,
    /// Put the UTF-8 text on the clipboard: a text field's `cut` (SPEC
    /// §10.2).
    pub clipboard: Option<extern "C" fn(ctx: *mut c_void, text: *const u8, len: usize)>,
    /// A text field's `paste` (SPEC §10.2): read the clipboard and give its
    /// text to `hotty_host_paste`. Null: the terminal does not read it, and
    /// the key reaches the program.
    pub paste: Option<extern "C" fn(ctx: *mut c_void)>,
    /// `a=edit` (SPEC §10.6): the edit commands the program takes on the
    /// screen it is on, in place of the last ones, comma-separated (empty:
    /// none). Keep them per screen; a key of one goes to the program as
    /// `hotty_host_command`.
    pub edit: Option<extern "C" fn(ctx: *mut c_void, names: *const u8, len: usize)>,
    /// Show the level of the program's menu that its keys reach, the
    /// surface `surface`, `cols`×`rows` cells, at the bottom right of the
    /// viewport, above everything (SPEC §10.7). Its pixels come from
    /// `hotty_host_render`; `remove` hides it.
    pub menu: Option<extern "C" fn(ctx: *mut c_void, surface: *const c_char, cols: u16, rows: u16)>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct HottyRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Called per rendered surface. `rgba` is the whole frame (`w`×`h`,
/// **premultiplied** RGBA8, `stride` bytes per row), owned by hotty-blitz and valid
/// only during the call; only the `nrects` rectangles changed. `full` is set
/// when every pixel changed (then there is one rectangle, the frame). A host
/// copies (and converts, if it wants straight alpha) just those rectangles.
pub type HottyFrameFn = extern "C" fn(
    ctx: *mut c_void,
    surface: *const c_char,
    w: u32,
    h: u32,
    rgba: *const u8,
    stride: usize,
    rects: *const HottyRect,
    nrects: usize,
    full: bool,
);

pub struct HottyHost {
    host: Host,
    /// Set after a panic the host could not recover from (see `guard`):
    /// it stops doing anything rather than risk running on broken state
    /// inside the terminal.
    dead: bool,
    /// Decodes what `hotty_host_osc` is handed, joining chunks.
    decoder: Decoder,
    /// The name returned by `hotty_host_focused`, kept alive here.
    focused: Option<CString>,
    /// The name returned by `hotty_host_cursor`.
    cursor: Option<CString>,
    /// The url returned by `hotty_host_hyperlink`.
    hyperlink: Option<CString>,
    /// The text returned by `hotty_host_selected_text`.
    selected: Option<CString>,
}

/// Runs `f` on a live host, catching panics at the C boundary. A panic
/// costs the surface whose document the host was working in (`Host::recover`;
/// all of them when it was in none), not the host: the other surfaces keep
/// drawing and taking commands. Only a panic while recovering disables it.
fn guard<R>(h: *mut HottyHost, default: R, f: impl FnOnce(&mut HottyHost) -> R) -> R {
    let Some(host) = (unsafe { h.as_mut() }) else {
        return default;
    };
    if host.dead {
        return default;
    }
    host.host.leave();
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(unsafe { &mut *h }))) {
        Ok(r) => r,
        Err(_) => {
            let host = unsafe { &mut *h };
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| host.host.recover())) {
                Ok(Some(name)) => {
                    eprintln!("hotty-blitz: surface {name} panicked and is deleted")
                }
                Ok(None) => eprintln!("hotty-blitz: host panicked; every surface is deleted"),
                Err(_) => {
                    eprintln!("hotty-blitz: host panicked; hotty is disabled for this terminal");
                    host.dead = true;
                }
            }
            default
        }
    }
}

fn rgb(v: u32) -> Rgb {
    Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

unsafe fn config(c: &HottyConfig) -> Config {
    let font = if c.font_family.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(c.font_family) }
            .to_string_lossy()
            .into_owned()
    };
    Config {
        metrics: Metrics {
            cell_w: c.cell_w.max(1),
            cell_h: c.cell_h.max(1),
            scale: if c.scale > 0.0 { c.scale } else { 1.0 },
        },
        theme: Theme {
            fg: rgb(c.fg),
            bg: rgb(c.bg),
            palette: c.palette.map(rgb),
            dark: c.dark,
        },
        font_family: font,
        font_size: (c.font_size > 0.0).then_some(c.font_size),
        ..Config::default()
    }
}

fn run_effects(effects: Vec<Effect>, fx: Option<&HottyEffects>) {
    let Some(fx) = fx else { return };
    for e in effects {
        match e {
            Effect::Reply(b) => {
                if let Some(f) = fx.reply {
                    f(fx.ctx, b.as_ptr(), b.len());
                }
            }
            Effect::Place {
                surface,
                cols,
                rows,
                window: w,
                z,
                created,
                move_cursor,
            } => {
                if let (Some(f), Ok(name)) = (fx.place, CString::new(surface)) {
                    f(fx.ctx, name.as_ptr(), cols, rows, w.x, w.y, w.w, w.h, z as i32, created, move_cursor);
                }
            }
            Effect::Delete { surface } => {
                if let (Some(f), Ok(name)) = (fx.remove, CString::new(surface)) {
                    f(fx.ctx, name.as_ptr());
                }
            }
            Effect::Hide { surface } => {
                if let (Some(f), Ok(name)) = (fx.hide, CString::new(surface)) {
                    f(fx.ctx, name.as_ptr());
                }
            }
            Effect::Clipboard(text) => {
                if let Some(f) = fx.clipboard {
                    f(fx.ctx, text.as_ptr(), text.len());
                }
            }
            Effect::Paste => {
                if let Some(f) = fx.paste {
                    f(fx.ctx);
                }
            }
            Effect::Edit(names) => {
                if let Some(f) = fx.edit {
                    let names = names.join(",");
                    f(fx.ctx, names.as_ptr(), names.len());
                }
            }
            Effect::Menu { surface, cols, rows } => {
                if let (Some(f), Ok(name)) = (fx.menu, CString::new(surface)) {
                    f(fx.ctx, name.as_ptr(), cols, rows);
                }
            }
        }
    }
}

/// # Safety
/// `cfg` must point to a valid `HottyConfig`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_new(cfg: *const HottyConfig) -> *mut HottyHost {
    let Some(cfg) = (unsafe { cfg.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let host = Host::new(unsafe { config(cfg) });
    Box::into_raw(Box::new(HottyHost {
        host,
        dead: false,
        decoder: Decoder::with_max(crate::MAX_COMMAND_BYTES),
        focused: None,
        cursor: None,
        hyperlink: None,
        selected: None,
    }))
}

/// # Safety
/// `h` must come from `hotty_host_new` and not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_free(h: *mut HottyHost) {
    if !h.is_null() {
        drop(unsafe { Box::from_raw(h) });
    }
}

/// New cell size, theme or font: every surface re-renders at the new size.
/// A new font changes a cell in CSS pixels (SPEC §5.3), and a program that
/// asked for the capabilities hears it, as it hears a new theme, as `caps`
/// (SPEC §4, §9): with `hotty_host_events`, after the render. A zoom tells
/// it nothing.
///
/// # Safety
/// `h` and `cfg` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_configure(h: *mut HottyHost, cfg: *const HottyConfig) {
    let Some(cfg) = (unsafe { cfg.as_ref() }) else {
        return;
    };
    let mut c = unsafe { config(cfg) };
    guard(h, (), |h| {
        c.resource_quota = h.host.config().resource_quota;
        h.host.set_config(c);
    })
}

/// One OSC 7279 body: everything after `7279;` (control, then `;` and the
/// payload). Chunked commands are reassembled across calls.
///
/// # Safety
/// `h` must be valid; `body` must point to `len` readable bytes; `fx` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_osc(
    h: *mut HottyHost,
    body: *const u8,
    len: usize,
    fx: *const HottyEffects,
) {
    let body = if body.is_null() {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(body, len) }
    };
    let fx = unsafe { fx.as_ref() };
    guard(h, (), |h| {
        let mut framed = Vec::with_capacity(len + 12);
        framed.extend_from_slice(b"\x1b]");
        framed.extend_from_slice(hotty::NUMBER.as_bytes());
        framed.push(b';');
        framed.extend_from_slice(body);
        framed.extend_from_slice(b"\x1b\\");
        if let Fed::Complete(c) = h.decoder.feed(&framed) {
            let effects = h.host.handle(&c);
            run_effects(effects, fx);
        }
    })
}

/// # Safety
/// `h` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_has_dirty(h: *const HottyHost) -> bool {
    unsafe { h.as_ref() }.is_some_and(|h| h.host.has_dirty())
}

/// Milliseconds until an animated image (GIF, APNG, WebP) on a placed
/// surface shows its next frame, scrollbars that fade are drawn again, or a
/// scroll that held a `view` back comes to rest (SPEC §9.5): render then
/// (`hotty_host_has_dirty` is true by that time). 0 when one is due now;
/// -1 when nothing plays, or nothing that plays is in a placement's
/// window, no scrollbar fades and no `view` waits. Ask again after every
/// call that renders, handles commands or hands the host input, and keep
/// one timer: each has one deadline at a time.
///
/// # Safety
/// `h` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_next_frame(h: *mut HottyHost) -> i64 {
    guard(h, -1, |h| match h.host.next_frame() {
        None => -1,
        Some(t) => {
            let left = t.saturating_duration_since(std::time::Instant::now());
            // Rounded up: a timer that fires early finds nothing due.
            left.as_micros().div_ceil(1000).min(i64::MAX as u128) as i64
        }
    })
}

/// The next render delivers `surface`'s whole frame, changed or not: for an
/// adapter that must show it anew and no longer has its pixels.
///
/// # Safety
/// `h` must be valid; `surface` a NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_redeliver(h: *mut HottyHost, surface: *const c_char) {
    let Some(surface) = (unsafe { name(surface) }) else {
        return;
    };
    guard(h, (), |h| h.host.redeliver(surface))
}

/// Renders every placed surface whose document changed.
///
/// # Safety
/// `h` must be valid; `cb` is called synchronously with pointers valid only
/// for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_render(h: *mut HottyHost, ctx: *mut c_void, cb: HottyFrameFn) {
    guard(h, (), |h| {
        h.host.render_dirty(&mut |name, frame, damage| {
            let Ok(name) = CString::new(name) else { return };
            let (rects, full): (Vec<HottyRect>, bool) = match damage {
                Damage::Full => (vec![rect(frame.full())], true),
                Damage::Rects(rs) => (rs.iter().map(|r| rect(*r)).collect(), false),
            };
            cb(
                ctx,
                name.as_ptr(),
                frame.width,
                frame.height,
                frame.rgba.as_ptr(),
                frame.width as usize * 4,
                rects.as_ptr(),
                rects.len(),
                full,
            );
        })
    })
}

/// Sends what rendering and the terminal's settings found for the program
/// through `fx`: `caps` events (SPEC §4, §9) after `hotty_host_configure`,
/// `hotty_host_set_network`, `hotty_host_set_passthrough` or
/// `hotty_host_set_counts` changed what the capabilities say, `fit` events
/// (SPEC §5.2), with the rows of the last frame drawn, and `view` events
/// (§9.5), with what it shows, at most one of each per surface, and a
/// `caret` event (§9) held back to this frame. Call after
/// `hotty_host_render`.
///
/// # Safety
/// `h` must be valid; `fx` may be null (the events are dropped).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_events(h: *mut HottyHost, fx: *const HottyEffects) {
    let fx = unsafe { fx.as_ref() };
    guard(h, (), |h| run_effects(h.host.take_events(), fx))
}

fn rect(r: crate::Rect) -> HottyRect {
    HottyRect {
        x: r.x,
        y: r.y,
        w: r.w,
        h: r.h,
    }
}

/// Drops every surface (full reset), the program's menus, and the touch
/// areas of both screens (SPEC §9.6). Deletions arrive through `fx`.
///
/// # Safety
/// `h` must be valid; `fx` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_reset(h: *mut HottyHost, fx: *const HottyEffects) {
    let fx = unsafe { fx.as_ref() };
    guard(h, (), |h| run_effects(h.host.reset(), fx))
}

unsafe fn name<'a>(s: *const c_char) -> Option<&'a str> {
    if s.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(s) }.to_str().ok()
}

fn mods(bits: u32) -> Mods {
    Mods {
        shift: bits & 1 != 0,
        ctrl: bits & 2 != 0,
        alt: bits & 4 != 0,
        super_: bits & 8 != 0,
    }
}

/// A pointer event at surface pixel `(x, y)`. `kind`: 0 move, 1 down, 2 up, 3 leave;
/// down and up are the primary button's (SPEC §10.1): pass no other button,
/// and no touch, which goes to `hotty_host_touch`. `mods`: 1 shift, 2 ctrl,
/// 4 alt, 8 super. A press also takes the keyboard
/// from any other surface that has it (SPEC §10.1): its events come through
/// `fx` too. From a press to its release the pointer is the surface's, for
/// drags (SPEC §9.1): every move comes here, with `(x, y)` counted from its
/// top left even outside it; `leave` before the release ends a drag.
/// A surface with `v=1` hears `hover` (SPEC §9.4) from moves, releases and
/// leaves: pass `leave` whenever the pointer goes off the surface (onto the
/// cells, another surface, a part it lets through, or out of the window).
/// `count`, for down and up: the platform's count of the press in a run of
/// clicks (GTK's n_press, AppKit's clickCount), 1 for a single one, which
/// the clicks the release makes carry (SPEC §9); 0 when it does not know.
/// Call `hotty_host_set_counts` once it passes them.
///
/// # Safety
/// `h` and `surface` must be valid; `fx` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_pointer(
    h: *mut HottyHost,
    surface: *const c_char,
    kind: u32,
    x: f32,
    y: f32,
    mod_bits: u32,
    count: u32,
    fx: *const HottyEffects,
) {
    let Some(surface) = (unsafe { name(surface) }) else {
        return;
    };
    let fx = unsafe { fx.as_ref() };
    let kind = match kind {
        1 => PointerKind::Down,
        2 => PointerKind::Up,
        3 => PointerKind::Leave,
        _ => PointerKind::Move,
    };
    guard(h, (), |h| {
        run_effects(h.host.pointer_counted(surface, kind, x, y, mods(mod_bits), count), fx)
    })
}

/// A wheel's turn, a touchpad's scroll or a touch drag at device pixel
/// `(x, y)` of the surface, by `(dx, dy)` device pixels: positive scrolls
/// towards the content's end, right and down, and a wheel's notch as far as
/// it scrolls the cells. `surface` null or empty: over the cells. `mods` as
/// for a pointer event; shift turns a vertical wheel horizontal. Returns
/// true if the surface took it: its document scrolled, or
/// `overscroll-behavior` stopped it there (SPEC §5.3). Otherwise the host
/// handles it as over the cells beneath (§9): scrollback, or the program's
/// wheel input. Its events (`hover`) come through `fx`; the `view` it
/// changes (§9.5) waits for the gesture to come to rest
/// (`hotty_host_next_frame`).
///
/// # Safety
/// `h` must be valid; `surface` may be null; `fx` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_wheel(
    h: *mut HottyHost,
    surface: *const c_char,
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    mod_bits: u32,
    fx: *const HottyEffects,
) -> bool {
    let surface = unsafe { name(surface) }.unwrap_or("");
    let fx = unsafe { fx.as_ref() };
    guard(h, false, |h| {
        let out = h.host.wheel(surface, x, y, dx, dy, mods(mod_bits));
        run_effects(out.effects, fx);
        out.taken
    })
}

/// A finger on `surface`, the surface its touch began on, at device pixel
/// `(x, y)` of it, counted from its top left even outside it (negative, or
/// past its size). Pass every phase of a touch that begins over a surface
/// (where `hotty_host_takes_pointer` is true) here, down first, and none of
/// it to `hotty_host_pointer`. `phase`: 0 down (the first finger touched),
/// 1 move, 2 up (it lifted), 3 cancel (a second finger touched, or the
/// platform cancelled the touch), 4 long press (the terminal took the touch
/// for one). `mods` as for a pointer event. Returns
/// whose the touch is: 0 the terminal's (it scrolls with it, through
/// `hotty_host_wheel` first, or takes it for a long press, as with any
/// touch), 1 undecided (it may still drag: hold its moves), 2 the surface's
/// (a drag, or on up a tap, which the surface took as a click where the
/// finger lifted: do nothing with it). A touch drags an element that opts in
/// when its `touch-action` allows no pan along the touch's first move past 8
/// CSS pixels (SPEC §9.1); its events come through `fx`. See `Host::touch`.
///
/// # Safety
/// `h` must be valid; `surface` may be null; `fx` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_touch(
    h: *mut HottyHost,
    surface: *const c_char,
    phase: u32,
    x: f32,
    y: f32,
    mod_bits: u32,
    fx: *const HottyEffects,
) -> u32 {
    let surface = unsafe { name(surface) }.unwrap_or("");
    let fx = unsafe { fx.as_ref() };
    let phase = match phase {
        0 => TouchPhase::Down,
        1 => TouchPhase::Move,
        2 => TouchPhase::Up,
        3 => TouchPhase::Cancel,
        4 => TouchPhase::LongPress,
        _ => return 0,
    };
    guard(h, 0, |h| {
        let out = h.host.touch(surface, phase, x, y, mods(mod_bits));
        run_effects(out.effects, fx);
        match out.touch {
            Touch::Terminal => 0,
            Touch::Undecided => 1,
            Touch::Surface => 2,
        }
    })
}

/// A report of the mouse's primary button for a finger in the cells (SPEC
/// §9.6), which the terminal makes as it reports the mouse's: in the
/// program's mode and encoding. See `hotty_host_cells_touch`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HottyMouse {
    /// 0 a move with the button held, 1 the press, 2 the release.
    pub kind: u32,
    /// Device pixels from the top left of the screen's cells.
    pub x: f32,
    pub y: f32,
    /// The screen's cell, from 0 at its top left: negative, or past the
    /// screen, where the finger is off it.
    pub c: i32,
    pub r: i32,
    /// As for pointer events: 1 shift, 2 ctrl, 4 alt, 8 super. Leave out
    /// one with which the terminal keeps a mouse press for itself, such as
    /// shift where shift and a press select (SPEC §9.6).
    pub mods: u32,
}

/// The most reports one call of `hotty_host_cells_touch` makes.
pub const HOTTY_MOUSE_MAX: usize = 4;

fn mod_bits(m: Mods) -> u32 {
    u32::from(m.shift) | u32::from(m.ctrl) << 1 | u32::from(m.alt) << 2 | u32::from(m.super_) << 3
}

/// A finger on the cells, at device pixel `(x, y)` from the top left of the
/// screen's cells, even outside them. Pass every phase of a touch that
/// begins on the cells here (outside every surface, or where one lets the
/// pointer through), down first; one that begins over a surface goes to
/// `hotty_host_touch`. `phase` and `mods` as for `hotty_host_touch`.
/// `reports`: the program has the terminal report the mouse's buttons, in
/// any mode; `at_screen`: the terminal shows the screen, not its
/// scrollback. Both are read at down: only then does an area the program
/// named with `a=touch` apply (SPEC §9.6). Returns whose the touch is:
/// 0 the terminal's (it scrolls, taps or long-presses with it, as with any
/// touch), 1 undecided (it began in an area and may still drag: hold its
/// moves, and take it for no tap), 2 the program's mouse (do nothing with
/// it but the reports). The reports to make, in order, at most
/// `HOTTY_MOUSE_MAX`, are written to `mouse`, and their count to `nmouse`:
/// a drag's press where the touch began and its moves and release, a tap's
/// press and release where it lifted, and the release a cancel makes. The
/// events a press on the cells makes (`blur`, SPEC §10.1) come through `fx`.
/// See `Host::cells_touch`.
///
/// # Safety
/// `h` must be valid; `mouse` null or room for `HOTTY_MOUSE_MAX`; `nmouse`
/// may be null; `fx` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_cells_touch(
    h: *mut HottyHost,
    phase: u32,
    x: f32,
    y: f32,
    mod_bits_in: u32,
    reports: bool,
    at_screen: bool,
    mouse: *mut HottyMouse,
    nmouse: *mut usize,
    fx: *const HottyEffects,
) -> u32 {
    if let Some(n) = unsafe { nmouse.as_mut() } {
        *n = 0;
    }
    let fx = unsafe { fx.as_ref() };
    let phase = match phase {
        0 => TouchPhase::Down,
        1 => TouchPhase::Move,
        2 => TouchPhase::Up,
        3 => TouchPhase::Cancel,
        4 => TouchPhase::LongPress,
        _ => return 0,
    };
    guard(h, 0, |h| {
        let out = h.host.cells_touch(phase, x, y, mods(mod_bits_in), reports, at_screen);
        run_effects(out.effects, fx);
        if !mouse.is_null() {
            let to = unsafe { std::slice::from_raw_parts_mut(mouse, HOTTY_MOUSE_MAX) };
            let clamp = |v: i64| v.clamp(i32::MIN.into(), i32::MAX.into()) as i32;
            for (slot, m) in to.iter_mut().zip(&out.mouse) {
                *slot = HottyMouse {
                    kind: match m.kind {
                        PointerKind::Down => 1,
                        PointerKind::Up => 2,
                        PointerKind::Move | PointerKind::Leave => 0,
                    },
                    x: m.x,
                    y: m.y,
                    c: clamp(m.c),
                    r: clamp(m.r),
                    mods: mod_bits(m.mods),
                };
            }
            if let Some(n) = unsafe { nmouse.as_mut() } {
                *n = out.mouse.len().min(HOTTY_MOUSE_MAX);
            }
        }
        match out.touch {
            CellsTouch::Terminal => 0,
            CellsTouch::Undecided => 1,
            CellsTouch::Mouse => 2,
        }
    })
}

/// The wheel gesture under way ended (a touchpad's fingers lifted, or its
/// momentum stopped): the next wheel begins another. Without it, a gesture
/// ends 150 ms after its last wheel, or at a press.
///
/// # Safety
/// `h` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_end_gesture(h: *mut HottyHost) {
    guard(h, (), |h| h.host.end_gesture())
}

/// A key for the focused surface as the user pressed it, offered first,
/// before the terminal's own shortcuts and the bindings that translate keys
/// (SPEC §10.4). A press has up to two names, UTF-8, as myelin key names
/// write them, from the key event, with the platform's command key as
/// `super`:
/// - `physical`: the physical key, with the modifiers held
///   (`ctrl+key_w`, `super+arrow_left`, `backspace`); or, for a modifier
///   tapped alone and named on its release, its sided key (`shift_left`);
/// - `other`: the character it types, with the modifiers (`ctrl+w`,
///   `super+c`); or, for that modifier, the modifier key (`shift`). Empty
///   for a key that types no character, such as `arrow_left`.
///
/// Either may be empty (`*_len` 0, the pointer may then be null), not both.
/// The host looks the physical name up first: the surface uses the press if
/// it uses either name, and where a keymap binds both, the physical binding
/// wins. For keys that type no text. Returns true if the surface used it;
/// its events then go through `fx`, and the terminal does nothing more with
/// the key. On false nothing happened: the terminal goes on, with its
/// shortcuts and then [`hotty_host_key_bytes`].
///
/// # Safety
/// `h` must be valid; `physical` and `other` must hold `physical_len` and
/// `other_len` bytes (a pointer may be null when its length is 0); `fx` may
/// be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_key_pressed(
    h: *mut HottyHost,
    physical: *const u8,
    physical_len: usize,
    other: *const u8,
    other_len: usize,
    fx: *const HottyEffects,
) -> bool {
    let fx = unsafe { fx.as_ref() };
    let text = |p: *const u8, n: usize| -> Option<&str> {
        if p.is_null() || n == 0 {
            return Some("");
        }
        std::str::from_utf8(unsafe { std::slice::from_raw_parts(p, n) }).ok()
    };
    let (Some(physical), Some(other)) = (text(physical, physical_len), text(other, other_len)) else {
        return false;
    };
    if physical.is_empty() && other.is_empty() {
        return false;
    }
    guard(h, false, |h| {
        let Some(surface) = h.host.focused_surface().map(str::to_string) else {
            return false;
        };
        h.host.set_paste(fx.is_some_and(|f| f.paste.is_some()));
        let outcome = h.host.key_pressed(&surface, physical, other);
        run_effects(outcome.effects, fx);
        outcome.consumed
    })
}

/// A key for the focused surface, as the bytes the terminal would send the
/// program for it (SPEC §10.4): its key encoding, in the modes the program
/// set, or what a binding writes. Returns true if the surface used any of
/// the keys in them; the others then reach the program through `fx`'s
/// replies, in order. On false the host sends the bytes to the program as
/// usual.
///
/// # Safety
/// `h` must be valid; `data` must hold `len` bytes (it may be null when
/// `len` is 0); `fx` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_key_bytes(
    h: *mut HottyHost,
    data: *const u8,
    len: usize,
    fx: *const HottyEffects,
) -> bool {
    let fx = unsafe { fx.as_ref() };
    if data.is_null() || len == 0 {
        return false;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    guard(h, false, |h| {
        let Some(surface) = h.host.focused_surface().map(str::to_string) else {
            return false;
        };
        h.host.set_paste(fx.is_some_and(|f| f.paste.is_some()));
        let outcome = h.host.key_bytes(&surface, bytes);
        run_effects(outcome.effects, fx);
        outcome.consumed
    })
}

/// Takes the keyboard from `surface` (a click landed elsewhere).
///
/// # Safety
/// `h` and `surface` must be valid; `fx` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_blur(
    h: *mut HottyHost,
    surface: *const c_char,
    fx: *const HottyEffects,
) {
    let Some(surface) = (unsafe { name(surface) }) else {
        return;
    };
    let fx = unsafe { fx.as_ref() };
    guard(h, (), |h| run_effects(h.host.blur(surface), fx))
}

/// The surface holding the keyboard, or null. Valid until the next call.
///
/// # Safety
/// `h` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_focused(h: *mut HottyHost) -> *const c_char {
    guard(h, std::ptr::null(), |h| {
        h.focused = h.host.focused_surface().and_then(|s| CString::new(s).ok());
        h.focused.as_ref().map_or(std::ptr::null(), |c| c.as_ptr())
    })
}

/// The pointer's shape over `surface` after the last pointer event, as a CSS
/// `cursor` name ("pointer", "text", ...), or NULL for the host's own.
/// Valid until the next call.
///
/// # Safety
/// `h` must be valid, `surface` NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_cursor(h: *mut HottyHost, surface: *const c_char) -> *const c_char {
    let Some(surface) = (unsafe { name(surface) }) else {
        return std::ptr::null();
    };
    guard(h, std::ptr::null(), |h| {
        h.cursor = h.host.cursor(surface).and_then(|c| CString::new(c).ok());
        h.cursor.as_ref().map_or(std::ptr::null(), |c| c.as_ptr())
    })
}

/// The hyperlink under the pointer on `surface` after the last pointer
/// event (SPEC §9: a link with `target="_blank"`), as its url, or NULL. The
/// terminal treats it as it treats an OSC 8 hyperlink: its gesture, its
/// feedback, its policies. Valid until the next call.
///
/// # Safety
/// `h` must be valid, `surface` NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_hyperlink(h: *mut HottyHost, surface: *const c_char) -> *const c_char {
    let Some(surface) = (unsafe { name(surface) }) else {
        return std::ptr::null();
    };
    guard(h, std::ptr::null(), |h| {
        h.hyperlink = h.host.hyperlink(surface).and_then(|u| CString::new(u).ok());
        h.hyperlink.as_ref().map_or(std::ptr::null(), |c| c.as_ptr())
    })
}

/// The text selected in the host, UTF-8, or NULL when nothing is (SPEC
/// §11): the selection of a surface's document, or of the focused text
/// field, never a password field's. The terminal's copy takes it when the
/// terminal has no selection of its own, and its copy on select after a
/// press's release. Valid until the next call.
///
/// # Safety
/// `h` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_selected_text(h: *mut HottyHost) -> *const c_char {
    guard(h, std::ptr::null(), |h| {
        h.selected = h
            .host
            .selected_text()
            .and_then(|t| CString::new(t.replace('\0', "")).ok());
        h.selected.as_ref().map_or(std::ptr::null(), |c| c.as_ptr())
    })
}

/// A press's two names from the C ABI (SPEC §10.4), each possibly empty;
/// `None` when both are or either is not UTF-8.
unsafe fn press<'a>(p: *const u8, pn: usize, o: *const u8, on: usize) -> Option<(&'a str, &'a str)> {
    let text = |p: *const u8, n: usize| -> Option<&'a str> {
        if p.is_null() || n == 0 {
            return Some("");
        }
        std::str::from_utf8(unsafe { std::slice::from_raw_parts(p, n) }).ok()
    };
    let (p, o) = (text(p, pn)?, text(o, on)?);
    (!p.is_empty() || !o.is_empty()).then_some((p, o))
}

/// A key as pressed, for the selection in a surface's document outside a
/// text field (SPEC §11), offered before the surface that has the keyboard
/// and the terminal's own shortcuts, whether or not a surface has the
/// keyboard. It has up to two names, as for [`hotty_host_key_pressed`]
/// (`physical`, `other`; either may be empty, not both); the selection's
/// keymap goes by the first it binds, the physical one first. 0: no surface
/// has such a selection, and the key goes on. 1: the selection's keymap
/// binds the key to `copy`; `*text` is the text to put on the clipboard,
/// valid until the next call, and the key goes no further. 2: it does not,
/// and the key goes on as if nothing were selected, the terminal copying
/// nothing with it.
///
/// # Safety
/// `h` must be valid, `physical` and `other` must point to their lengths in
/// bytes (null when 0), and `text` must be null or valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_copy_key(
    h: *mut HottyHost,
    physical: *const u8,
    physical_len: usize,
    other: *const u8,
    other_len: usize,
    text: *mut *const c_char,
) -> c_int {
    let Some((physical, other)) = (unsafe { press(physical, physical_len, other, other_len) }) else {
        return 0;
    };
    guard(h, 0, |h| match h.host.copy_key(physical, other) {
        CopyKey::None => 0,
        CopyKey::Other => 2,
        CopyKey::Copy(t) => {
            h.selected = CString::new(t.replace('\0', "")).ok();
            if let (Some(out), Some(c)) = (unsafe { text.as_mut() }, h.selected.as_ref()) {
                *out = c.as_ptr();
            }
            1
        }
    })
}

/// Types the UTF-8 `text` into the focused text field of the surface that
/// has the keyboard, as a field's `paste` does (SPEC §10.2, §11): the
/// clipboard read for `fx.paste`, or the terminal's own paste, which then
/// does not reach the program. False when no text field has the keyboard:
/// the terminal pastes to the program as usual. Its events go through `fx`.
///
/// # Safety
/// `h` must be valid, `text` must point to `len` bytes, and `fx` must be
/// null or valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_paste(
    h: *mut HottyHost,
    text: *const u8,
    len: usize,
    fx: *const HottyEffects,
) -> bool {
    let fx = unsafe { fx.as_ref() };
    let text = if text.is_null() || len == 0 {
        String::new()
    } else {
        String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(text, len) }).into_owned()
    };
    guard(h, false, |h| match h.host.paste(&text) {
        Some(effects) => {
            run_effects(effects, fx);
            true
        }
        None => false,
    })
}

/// Drops every surface's selection: there is one selection across the
/// terminal (SPEC §11), and the user pressed on the cells or selected
/// there. A press on a surface drops the others' itself.
///
/// # Safety
/// `h` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_clear_selection(h: *mut HottyHost) {
    guard(h, (), |h| h.host.clear_selection(None))
}

/// The host's half of the network policy (SPEC §7.2), in CSP's syntax:
/// `img-src https://example.com; font-src https:`. Null or empty: none, as
/// a new host starts. The capabilities report it (`net`).
///
/// # Safety
/// `h` must be valid; `policy` null or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_set_network(h: *mut HottyHost, policy: *const c_char) {
    let policy = if policy.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(policy) }.to_string_lossy().into_owned()
    };
    guard(h, (), |h| h.host.set_network(&policy))
}

/// A context pointer the terminal vouches for on any thread.
struct WakeCtx(*mut c_void);
unsafe impl Send for WakeCtx {}
unsafe impl Sync for WakeCtx {}

impl WakeCtx {
    /// The pointer, through the whole (Send) value: a closure that named
    /// the field would capture the bare pointer.
    fn ptr(&self) -> *mut c_void {
        self.0
    }
}

/// `wake(ctx)` is called on a fetching thread when something a document
/// fetched from the network arrives, or fails: render then, on the
/// terminal's own thread (`hotty_host_has_dirty` is true). It must return
/// quickly and not call into hotty-blitz. Null `wake` removes it. Once this
/// returns, the old one is not called again; nor is any after
/// `hotty_host_free`.
///
/// # Safety
/// `h` must be valid; `ctx` must stay valid for `wake` until it is
/// replaced or the host freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_set_waker(
    h: *mut HottyHost,
    wake: Option<extern "C" fn(ctx: *mut c_void)>,
    ctx: *mut c_void,
) {
    guard(h, (), |h| match wake {
        Some(wake) => {
            let ctx = WakeCtx(ctx);
            h.host.set_waker(move || wake(ctx.ptr()));
        }
        None => h.host.clear_waker(),
    })
}

/// The terminal lets the pointer pass through where a surface does not take
/// it (`hotty_host_takes_pointer`): the capabilities then list the feature
/// `passthrough` (SPEC §4, §9.3).
///
/// # Safety
/// `h` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_set_passthrough(h: *mut HottyHost, on: bool) {
    guard(h, (), |h| h.host.set_passthrough(on))
}

/// The terminal passes its platform's count of each press to
/// `hotty_host_pointer`: the capabilities list the feature `count` (SPEC
/// §4, §9).
///
/// # Safety
/// `h` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_set_counts(h: *mut HottyHost, on: bool) {
    guard(h, (), |h| h.host.set_counts(on))
}

/// Whether the surface takes the pointer at device pixel (`x`, `y`) of the
/// surface (Host::takes_pointer). Where it does not, the host hands the
/// pointer to what is below it (SPEC §9.3).
///
/// # Safety
/// `h` must be valid, `surface` NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_takes_pointer(h: *mut HottyHost, surface: *const c_char, x: f32, y: f32) -> bool {
    let Some(surface) = (unsafe { name(surface) }) else {
        return false;
    };
    guard(h, true, |h| h.host.takes_pointer(surface, x, y))
}

/// How the host reads keys (SPEC §10.2, §10.4–§10.6), from the terminal's
/// settings: `mod_super`, `mod` is `"super"` (macOS's Command) rather than
/// `"ctrl"` (SPEC §4);
/// `edit`, the edit shortcuts as a `data-keys` value of edit actions
/// (`ctrl+c=copy ctrl+insert=copy …`), null for the defaults;
/// `commands`, the terminal takes `a=edit` (`fx.edit`, `hotty_host_command`);
/// `menus`, it shows the program's menu (`fx.menu`), which the
/// capabilities then list as the feature `menu`. A program that asked for
/// the capabilities hears what changed (`caps`) through `fx`.
///
/// # Safety
/// `h` must be valid; `edit` null or NUL-terminated; `fx` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_set_keys(
    h: *mut HottyHost,
    mod_super: bool,
    edit: *const c_char,
    commands: bool,
    menus: bool,
    fx: *const HottyEffects,
) {
    let fx = unsafe { fx.as_ref() };
    let edit = (!edit.is_null()).then(|| unsafe { CStr::from_ptr(edit) }.to_string_lossy().into_owned());
    let k = crate::KeySettings {
        platform: hotty::keys::Platform {
            modifier: if mod_super { hotty::keys::Mod::Super } else { hotty::keys::Mod::Ctrl },
            edit: edit.map(|e| edit_shortcuts(&e)),
        },
        commands,
        menus,
    };
    guard(h, (), |h| {
        let effects = h.host.set_keys(k);
        run_effects(effects, fx);
    })
}

/// Edit shortcuts from a `data-keys` value: each edit action with the keys
/// bound to it, canonical, in SPEC §10.2's order. Other bindings, and keys
/// that do not parse, are left out.
fn edit_shortcuts(value: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = hotty::keys::EDIT_ACTIONS
        .iter()
        .map(|a| (a.to_string(), Vec::new()))
        .collect();
    for b in value.split_ascii_whitespace() {
        let Some((k, a)) = b.rsplit_once('=') else {
            continue;
        };
        let (Some(k), Some(slot)) = (hotty::keys::parse_key(k), out.iter_mut().find(|(x, _)| x == a)) else {
            continue;
        };
        if !slot.1.contains(&k) {
            slot.1.push(k);
        }
    }
    out
}

/// The edit command a press is (SPEC §10.6): the edit action whose shortcut
/// either of its names is (`physical`, `other`, UTF-8 as for
/// [`hotty_host_key_pressed`]; either may be empty, not both), the physical
/// one first. NUL-terminated and static; null for none.
///
/// # Safety
/// `h` must be valid; `physical` and `other` must hold their lengths in
/// bytes (null when 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_edit_command(
    h: *mut HottyHost,
    physical: *const u8,
    physical_len: usize,
    other: *const u8,
    other_len: usize,
) -> *const c_char {
    const NAMES: [&CStr; 6] = [c"select-all", c"copy", c"cut", c"paste", c"undo", c"redo"];
    let Some((physical, other)) = (unsafe { press(physical, physical_len, other, other_len) }) else {
        return std::ptr::null();
    };
    guard(h, std::ptr::null(), |h| {
        h.host
            .edit_command(physical, other)
            .and_then(|a| NAMES.iter().find(|n| n.to_bytes() == a.as_bytes()))
            .map_or(std::ptr::null(), |n| n.as_ptr())
    })
}

/// Sends the program in the cells the edit command `name` (SPEC §10.6),
/// through `fx`'s reply: from the key `key` (null from a menu), and, for a
/// paste, the clipboard's UTF-8 `text` (null when it holds no text or the
/// terminal cannot read it).
///
/// # Safety
/// `name` must be NUL-terminated; `key` null or `key_len` bytes; `text`
/// null or `text_len` bytes; `fx` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_command(
    name: *const c_char,
    key: *const u8,
    key_len: usize,
    text: *const u8,
    text_len: usize,
    fx: *const HottyEffects,
) {
    let fx = unsafe { fx.as_ref() };
    let Some(name) = (unsafe { self::name(name) }) else {
        return;
    };
    let bytes = |p: *const u8, n: usize| {
        (!p.is_null()).then(|| String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(p, n) }).into_owned())
    };
    let (key, text) = (bytes(key, key_len), bytes(text, text_len));
    let event = Host::command_event(name, key.as_deref(), text.as_deref());
    run_effects(vec![Effect::Reply(event)], fx);
}

/// A key that reaches the program in the cells, while a level of the
/// program's menu may show (SPEC §10.7): `escape`, and a key it does not
/// list, hide the level. The key has up to two names, UTF-8 as for
/// [`hotty_host_key_pressed`] (either may be empty, not both); it is listed
/// if the level lists either. The key still reaches the program.
///
/// # Safety
/// `h` must be valid; `physical` and `other` must hold their lengths in
/// bytes (null when 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_cells_key(
    h: *mut HottyHost,
    physical: *const u8,
    physical_len: usize,
    other: *const u8,
    other_len: usize,
) {
    let Some((physical, other)) = (unsafe { press(physical, physical_len, other, other_len) }) else {
        return;
    };
    guard(h, (), |h| h.host.cells_key(physical, other))
}

/// The terminal's screen changed, or is told for the first time (SPEC
/// §10.7): leaving the alternate screen drops its menu and its touch areas
/// (§9.6), and any change hides the level shown.
///
/// # Safety
/// `h` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_set_screen(h: *mut HottyHost, alt: bool) {
    guard(h, (), |h| h.host.set_screen(alt))
}

/// Picks the first item of the program's menu that has the UTF-8 name
/// `name`, depth-first (SPEC §10.7). If it is enabled, sends the program
/// `command` with exactly `{"name": name}` through `fx`'s reply, as
/// `hotty_host_command` does with no key, and returns true; else sends
/// nothing and returns false. For a list of commands the terminal already
/// has, where it may offer the menu's items; it draws none of its own for
/// them. A terminal routes a pick of `copy` or `paste` through its own Edit
/// menu (SPEC §10.6): the selection first, and the clipboard's text.
///
/// # Safety
/// `h` must be valid; `name` must hold `len` bytes; `fx` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_pick(h: *mut HottyHost, name: *const u8, len: usize, fx: *const HottyEffects) -> bool {
    let fx = unsafe { fx.as_ref() };
    if name.is_null() {
        return false;
    }
    let Ok(name) = std::str::from_utf8(unsafe { std::slice::from_raw_parts(name, len) }) else {
        return false;
    };
    guard(h, false, |h| match h.host.menu_pick(name) {
        Some(event) => {
            run_effects(vec![Effect::Reply(event)], fx);
            true
        }
        None => false,
    })
}


#[cfg(test)]
mod tests {
    use super::*;

    fn c_config(font_size: f32) -> HottyConfig {
        HottyConfig {
            cell_w: 16,
            cell_h: 34,
            scale: 2.0,
            fg: 0xffffff,
            bg: 0,
            palette: [0; 16],
            dark: true,
            font_family: std::ptr::null(),
            font_size,
        }
    }

    #[test]
    fn font_size_passes_through_and_zero_derives_it() {
        let given = unsafe { config(&c_config(14.0)) };
        assert_eq!(given.font_size, Some(14.0));
        assert!(crate::style::host_css(&given).contains("font-size: 14px;"));
        let derived = unsafe { config(&c_config(0.0)) };
        assert_eq!(derived.font_size, None);
        assert!(crate::style::host_css(&derived).contains("font-size: 13.6px;"));
    }

    /// A host with surfaces `a` and `b`, placed and drawn.
    fn two_surfaces() -> *mut HottyHost {
        let h = unsafe { hotty_host_new(&c_config(0.0)) };
        let host = unsafe { &mut (*h).host };
        for s in ["a", "b"] {
            let cmd = |pairs: &[(&str, &str)], payload: &str| {
                hotty::Message::new(
                    pairs.iter().copied().collect(),
                    payload.as_bytes().to_vec(),
                )
            };
            host.handle(&cmd(&[("a", "doc"), ("s", s), ("q", "2")], "<p>x</p>"));
            host.handle(&cmd(&[("a", "place"), ("s", s), ("c", "4"), ("r", "1"), ("q", "2")], ""));
        }
        host.render_dirty(&mut |_, _, _| {});
        h
    }

    fn names(h: *mut HottyHost) -> Vec<String> {
        unsafe { &(*h).host }.surface_names().map(String::from).collect()
    }

    #[test]
    fn a_panic_in_a_surface_costs_that_surface() {
        let h = two_surfaces();
        guard(h, (), |h| {
            crate::enter(&mut h.host.working, "a");
            panic!("a broken document");
        });
        let host = unsafe { &mut *h };
        assert!(!host.dead);
        assert_eq!(names(h), ["b"]);
        // Its picture goes with the next events; the host still has them due.
        assert!(host.host.has_dirty());
        let events = host.host.take_events();
        assert!(matches!(&events[..], [Effect::Delete { surface }] if surface == "a"));
        assert!(!host.host.has_dirty());
        // The other surface still takes commands.
        let ok = guard(h, false, |h| {
            h.host.handle(&hotty::Message::new(
                [("a", "delta"), ("s", "b"), ("q", "2")].into_iter().collect(),
                // {"ops": [{"op": "text", "t": "p", "v": "y"}]}
                b"\x81\xa3ops\x91\x83\xa2op\xa4text\xa1t\xa1p\xa1v\xa1y".to_vec(),
            ));
            true
        });
        assert!(ok);
        unsafe { hotty_host_free(h) };
    }

    #[test]
    fn a_finger_in_an_area_is_the_mouse_through_the_c_abi() {
        // Cells 16×34 device pixels, 2 device pixels per CSS pixel.
        let h = unsafe { hotty_host_new(&c_config(0.0)) };
        let body = rmp_serde::to_vec_named(&serde_json::json!({
            "areas": [{"c": 1, "r": 0, "w": 10, "h": 1, "drag": "x"}]
        }))
        .unwrap();
        unsafe { &mut (*h).host }.handle(&hotty::Message::new(
            [("a", "touch"), ("q", "2")].into_iter().collect(),
            body,
        ));
        let mut mouse = [HottyMouse::default(); HOTTY_MOUSE_MAX];
        let mut n = 9;
        let mut touch = |phase: u32, x: f32, mods: u32, reports: bool| unsafe {
            let r = hotty_host_cells_touch(h, phase, x, 17.0, mods, reports, true, mouse.as_mut_ptr(), &mut n, std::ptr::null());
            (r, mouse[..n].to_vec())
        };
        // Outside the area, and in it while the program reads no mouse: the
        // terminal's.
        assert_eq!(touch(0, 8.0, 0, true), (0, vec![]));
        assert_eq!(touch(0, 24.0, 0, false), (0, vec![]));
        // In it, with shift: undecided, then 20 CSS px across, the mouse's.
        assert_eq!(touch(0, 24.0, 1, true), (1, vec![]));
        let (r, got) = touch(1, 64.0, 2, true);
        assert_eq!(r, 2);
        let m = |kind, x, c, mods| HottyMouse { kind, x, y: 17.0, c, r: 0, mods };
        assert_eq!(got, [m(1, 24.0, 1, 1), m(0, 64.0, 4, 2)]);
        assert_eq!(touch(2, -40.0, 8, true), (2, vec![m(2, -40.0, -3, 8)]));
        // A phase there is no such: nothing.
        assert_eq!(touch(9, 24.0, 0, true), (0, vec![]));
        unsafe { hotty_host_free(h) };
    }

    #[test]
    fn a_panic_in_no_surface_costs_them_all_but_not_the_host() {
        let h = two_surfaces();
        // A call that names no surface: what an earlier call entered no
        // longer counts.
        guard(h, (), |h| crate::enter(&mut h.host.working, "a"));
        guard(h, (), |_| panic!("no surface's fault"));
        let host = unsafe { &mut *h };
        assert!(!host.dead);
        assert!(names(h).is_empty());
        assert_eq!(host.host.take_events().len(), 2);
        unsafe { hotty_host_free(h) };
    }
}

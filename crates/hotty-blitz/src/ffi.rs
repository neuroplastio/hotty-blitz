//! C ABI for hosts that are not Rust (the Ghostty fork, PoC-2; see
//! `include/hotty_blitz.h`). A host owns one `hotty_host` per terminal, calls it
//! from one thread at a time, and hands it OSC 7279 bodies as its parser
//! finishes them. Everything the host must do comes back through callbacks.

use crate::{Config, Damage, Effect, Host, Key, KeyName, Metrics, Mods, PointerKind, Rgb, Theme};
use hotty_wire::{Event, Scanner};
use std::ffi::{CStr, CString, c_char, c_void};

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
    /// Set after a panic: the host stops doing anything rather than risk
    /// running on broken state inside the terminal.
    dead: bool,
    scanner: Scanner,
    /// The name returned by `hotty_host_focused`, kept alive here.
    focused: Option<CString>,
    /// The name returned by `hotty_host_cursor`.
    cursor: Option<CString>,
    /// The url returned by `hotty_host_hyperlink`.
    hyperlink: Option<CString>,
}

/// Runs `f` on a live host, catching panics at the C boundary.
fn guard<R>(h: *mut HottyHost, default: R, f: impl FnOnce(&mut HottyHost) -> R) -> R {
    let Some(host) = (unsafe { h.as_mut() }) else {
        return default;
    };
    if host.dead {
        return default;
    }
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(unsafe { &mut *h }))) {
        Ok(r) => r,
        Err(_) => {
            eprintln!("hotty-blitz: host panicked; HOTTY is disabled for this terminal");
            host.dead = true;
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
        scanner: Scanner::new(),
        focused: None,
        cursor: None,
        hyperlink: None,
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
        framed.extend_from_slice(hotty_wire::OSC_NUMBER.as_bytes());
        framed.push(b';');
        framed.extend_from_slice(body);
        framed.extend_from_slice(b"\x1b\\");
        let mut commands = Vec::new();
        h.scanner.feed(&framed, &mut |e| {
            if let Event::Command(c) = e {
                commands.push(c);
            }
        });
        for c in commands {
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

fn rect(r: crate::Rect) -> HottyRect {
    HottyRect {
        x: r.x,
        y: r.y,
        w: r.w,
        h: r.h,
    }
}

/// Drops every surface (full reset). Deletions arrive through `fx`.
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
        meta: bits & 8 != 0,
    }
}

/// A pointer event at surface pixel `(x, y)`. `kind`: 0 move, 1 down, 2 up, 3 leave;
/// down and up are the primary button's, or a tap's (SPEC §10.1): pass no
/// other button. `mods`: 1 shift, 2 ctrl, 4 alt, 8 super. A press also takes the keyboard
/// from any other surface that has it (SPEC §10.1): its events come through
/// `fx` too. From a press to its release the pointer is the surface's, for
/// drags (SPEC §9.1): every move comes here, with `(x, y)` counted from its
/// top left even outside it; `leave` before the release ends a drag.
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
        run_effects(h.host.pointer(surface, kind, x, y, mods(mod_bits)), fx)
    })
}

/// A key for the focused surface. `key`: 0 text (in `text`), 1 enter, 2 tab,
/// 3 backspace, 4 delete, 5 escape, 6 left, 7 right, 8 up, 9 down, 10 home,
/// 11 end, 12 page up, 13 page down, 14 space. Returns true if the surface
/// used it; otherwise the host sends the key to the program as usual.
///
/// # Safety
/// `h` must be valid; `text` may be null; `fx` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_key(
    h: *mut HottyHost,
    key: u32,
    text: *const c_char,
    mod_bits: u32,
    fx: *const HottyEffects,
) -> bool {
    let fx = unsafe { fx.as_ref() };
    let text = unsafe { name(text) };
    guard(h, false, |h| {
        let Some(surface) = h.host.focused_surface().map(str::to_string) else {
            return false;
        };
        let name = match key {
            0 => match text {
                Some(t) if !t.is_empty() => KeyName::Char(t.to_string()),
                _ => return false,
            },
            1 => KeyName::Enter,
            2 => KeyName::Tab,
            3 => KeyName::Backspace,
            4 => KeyName::Delete,
            5 => KeyName::Escape,
            6 => KeyName::Left,
            7 => KeyName::Right,
            8 => KeyName::Up,
            9 => KeyName::Down,
            10 => KeyName::Home,
            11 => KeyName::End,
            12 => KeyName::PageUp,
            13 => KeyName::PageDown,
            14 => KeyName::Space,
            _ => KeyName::Other,
        };
        let outcome = h.host.key(
            &surface,
            &Key {
                name,
                mods: mods(mod_bits),
            },
        );
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

/// The terminal lets the pointer pass through where a surface does not take
/// it (`hotty_host_takes_pointer`): the capabilities then say
/// `"passthrough": true` (SPEC §4, §9.3).
///
/// # Safety
/// `h` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hotty_host_set_passthrough(h: *mut HottyHost, on: bool) {
    guard(h, (), |h| h.host.set_passthrough(on))
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
}

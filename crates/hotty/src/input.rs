//! Input routing for the polyfill (SPEC §10).
//!
//! The polyfill owns the real terminal's mouse modes, and it mirrors the
//! screen with a VT parser. The placeholder cells it printed are in that
//! mirror, carrying the surface's image id in their colour and their row in a
//! diacritic, so hit testing survives scrolling with no bookkeeping. Pointer
//! events over a surface go to its document. Everything else goes to the
//! program, re-encoded the way the program asked for it.
//!
//! Keys go to a surface only while it holds the keyboard. Keys the focused
//! element does not use are forwarded as the **original bytes**, which are
//! already in the encoding the program asked the terminal for. A key's
//! release, which the terminal reports when the program asked the kitty
//! keyboard protocol for event types, goes where its press went.

use crate::diacritics::DIACRITICS;
use hotty_blitz::{Effect, Host, Key, KeyName, Mods, PointerKind};
use std::collections::{BTreeSet, HashMap};

const MOUSE_MODES: [u16; 9] = [9, 1000, 1001, 1002, 1003, 1005, 1006, 1015, 1016];

pub struct InputRouter {
    cell_w: u32,
    cell_h: u32,
    /// Mouse modes the program set; the terminal's are ours.
    program: BTreeSet<u16>,
    /// Image id → surface name.
    ids: HashMap<u32, String>,
    vt: vt100::Parser,
    /// The surface under the pointer, for hover and leave.
    hovered: Option<String>,
    /// The surface that got the button press, which gets its release too.
    pressed: Option<String>,
    /// Where that surface's top left is on the screen, in pixels, as of the
    /// last report over it: a drag's moves off it are counted from there
    /// (SPEC §9.1).
    pressed_at: (f32, f32),
    /// A press with Alt held began the gesture under way: until its
    /// release, it is the program's (SPEC §9.2).
    program_press: bool,
    /// Keys whose press a surface took, unshifted: their releases are the
    /// surface's too, and the program never hears them.
    held: Vec<KeyName>,
    enabled: bool,
    partial: Vec<u8>,
}

impl InputRouter {
    pub fn new(cell_w: u32, cell_h: u32) -> InputRouter {
        let (rows, cols) = crate::term::winsize(std::os::fd::AsFd::as_fd(&std::io::stdout()))
            .map(|w| (w.ws_row, w.ws_col))
            .unwrap_or((24, 80));
        InputRouter {
            cell_w: cell_w.max(1),
            cell_h: cell_h.max(1),
            program: BTreeSet::new(),
            ids: HashMap::new(),
            vt: vt100::Parser::new(rows.max(1), cols.max(1), 0),
            hovered: None,
            pressed: None,
            pressed_at: (0.0, 0.0),
            program_press: false,
            held: Vec::new(),
            enabled: false,
            partial: Vec::new(),
        }
    }

    pub fn set_cell(&mut self, w: u32, h: u32) {
        self.cell_w = w.max(1);
        self.cell_h = h.max(1);
        if let Some(ws) = crate::term::winsize(std::os::fd::AsFd::as_fd(&std::io::stdout())) {
            self.vt
                .screen_mut()
                .set_size(ws.ws_row.max(1), ws.ws_col.max(1));
        }
    }

    /// Everything written to the terminal passes through here, so the mirror
    /// matches the screen.
    pub fn observe(&mut self, bytes: &[u8]) {
        self.vt.process(bytes);
    }

    /// Records a program's mouse-mode change. Returns true when the mode is
    /// the polyfill's to manage and must not reach the terminal as is.
    pub fn claims_mode(&mut self, mode: u16, set: bool) -> bool {
        if !MOUSE_MODES.contains(&mode) {
            return false;
        }
        if set {
            self.program.insert(mode);
        } else {
            self.program.remove(&mode);
        }
        true
    }

    pub fn reset(&mut self) {
        self.program.clear();
        self.ids.clear();
        self.hovered = None;
        self.pressed = None;
        self.held.clear();
        self.enabled = false;
    }

    pub fn placed(&mut self, surface: &str, id: u32) {
        self.ids.retain(|_, s| s != surface);
        self.ids.insert(id, surface.to_string());
    }

    pub fn removed(&mut self, surface: &str) {
        self.ids.retain(|_, s| s != surface);
    }

    /// The terminal reports all motion in SGR pixels while we listen.
    pub fn enable_on_terminal(&mut self, out: &mut Vec<u8>) {
        out.extend_from_slice(b"\x1b[?1003h\x1b[?1006h\x1b[?1016h");
        self.enabled = true;
    }

    pub fn disable_on_terminal(&mut self, out: &mut Vec<u8>) {
        out.extend_from_slice(b"\x1b[?1016l\x1b[?1006l\x1b[?1003l\x1b[?1002l\x1b[?1000l");
        self.enabled = false;
    }

    /// Input from the terminal. Bytes for the program are appended to
    /// `to_program`; the rest is handled here against `host`.
    pub fn feed(&mut self, input: &[u8], host: &mut Host, to_program: &mut Vec<u8>) {
        let mut data = std::mem::take(&mut self.partial);
        data.extend_from_slice(input);
        let mut i = 0;
        while i < data.len() {
            match parse_one(&data[i..]) {
                Parsed::Incomplete => {
                    self.partial = data[i..].to_vec();
                    return;
                }
                Parsed::Mouse {
                    len,
                    b,
                    x,
                    y,
                    press,
                } => {
                    self.mouse(b, x, y, press, host, to_program);
                    i += len;
                }
                Parsed::Key { len, key } => {
                    let raw = &data[i..i + len];
                    self.key(raw, key, host, to_program);
                    i += len;
                }
            }
        }
    }

    fn key(&mut self, raw: &[u8], key: Option<KeyIn>, host: &mut Host, to_program: &mut Vec<u8>) {
        let Some(KeyIn { key, base, release }) = key else {
            to_program.extend_from_slice(raw);
            return;
        };
        let held = self.held.iter().position(|k| *k == base);
        if release {
            // Wherever the keyboard is now: a surface may have taken it, or
            // given it back, between the press and the release.
            match held {
                Some(i) => {
                    self.held.swap_remove(i);
                }
                None => to_program.extend_from_slice(raw),
            }
            return;
        }
        if let Some(surface) = host.focused_surface().map(str::to_string) {
            let outcome = host.key(&surface, &key);
            push_effects(outcome.effects, to_program);
            if outcome.consumed {
                if held.is_none() {
                    self.held.push(base);
                }
                return;
            }
        }
        if let Some(i) = held {
            self.held.swap_remove(i);
        }
        to_program.extend_from_slice(raw);
    }

    /// Finds the surface at a pixel position, and the pixel inside it.
    fn hit(&self, px: u32, py: u32) -> Option<(String, f32, f32)> {
        let (col, row) = ((px / self.cell_w) as u16, (py / self.cell_h) as u16);
        let screen = self.vt.screen();
        let cell = screen.cell(row, col)?;
        let id = placeholder_id(cell)?;
        let name = self.ids.get(&id)?.clone();
        // Walk left to the row's first cell, which carries the diacritics.
        let mut first = col;
        while first > 0 {
            match screen.cell(row, first - 1).and_then(placeholder_id) {
                Some(i) if i == id => first -= 1,
                _ => break,
            }
        }
        let (srow, scol0) = diacritics(screen.cell(row, first)?)?;
        let scol = scol0 as u32 + (col - first) as u32;
        let x = scol * self.cell_w + px % self.cell_w;
        let y = srow as u32 * self.cell_h + py % self.cell_h;
        Some((name, x as f32, y as f32))
    }

    fn mouse(
        &mut self,
        b: u32,
        x: u32,
        y: u32,
        press: bool,
        host: &mut Host,
        to_program: &mut Vec<u8>,
    ) {
        // SGR-pixels coordinates are 1-based.
        let (px, py) = (x.saturating_sub(1), y.saturating_sub(1));
        let mods = Mods {
            shift: b & 4 != 0,
            alt: b & 8 != 0,
            ctrl: b & 16 != 0,
            meta: false,
        };
        let motion = b & 32 != 0;
        let wheel = b & 64 != 0;
        let button = b & 3;
        // A click is the primary button (SPEC §10.1): the others neither
        // press, release nor take the keyboard. SGR reports which button a
        // release is.
        let primary = button == 0 && !wheel;
        let target = self.hit(px, py).filter(|_| !wheel);
        if !motion {
            crate::log(&format!(
                "mouse b={b} px=({px},{py}) press={press} hit={target:?}"
            ));
        }
        let over = target.as_ref().map(|t| t.0.clone());

        // A press with Alt held is the program's, wherever it lands, and so
        // is its gesture until the release (SPEC §9.2): as on the cells.
        if press && !motion && primary {
            self.program_press = mods.alt;
        }
        if self.program_press {
            if let Some(old) = self.hovered.take() {
                push_effects(
                    host.pointer(&old, PointerKind::Leave, 0.0, 0.0, mods),
                    to_program,
                );
            }
            if press && !motion && primary {
                self.pressed = None;
                for other in host.surface_names().map(str::to_string).collect::<Vec<_>>() {
                    if host.is_focused(&other) {
                        push_effects(host.blur(&other), to_program);
                    }
                }
            } else if !press && !motion && primary {
                self.program_press = false;
            }
            self.forward_mouse(b, px, py, press, to_program);
            return;
        }

        // A wheel over a surface whose document scrolls is its own while
        // it can move that way (SPEC §5.3); otherwise, and over the cells,
        // it is the program's. One report is a row (or a column) of it.
        if wheel {
            let taken = match self.hit(px, py) {
                Some((surface, sx, sy)) => {
                    let (w, h) = (self.cell_w as f32, self.cell_h as f32);
                    let (dx, dy) = match button {
                        0 => (0.0, -h),
                        1 => (0.0, h),
                        2 => (-w, 0.0),
                        _ => (w, 0.0),
                    };
                    let out = host.wheel(&surface, sx, sy, dx, dy, mods);
                    push_effects(out.effects, to_program);
                    out.taken
                }
                // Over the cells: the gesture is the terminal's, even where
                // it goes on over a surface.
                None => host.wheel("", 0.0, 0.0, 0.0, 0.0, mods).taken,
            };
            if !taken {
                self.forward_mouse(b, px, py, press, to_program);
            }
            return;
        }

        // Hover: leave the old surface when the pointer moves off it.
        if self.hovered != over {
            if let Some(old) = self.hovered.take() {
                push_effects(
                    host.pointer(&old, PointerKind::Leave, 0.0, 0.0, mods),
                    to_program,
                );
            }
            self.hovered = over.clone();
        }

        // A drag that began on a surface stays with it until release.
        let owner = if self.pressed.is_some() && (motion || !press) {
            self.pressed.clone()
        } else {
            over.clone()
        };
        if let Some(surface) = owner {
            // Off the surface it holds, a drag goes on counting from its top
            // left: negative, or past its size (SPEC §9.1).
            let (sx, sy) = match &target {
                Some((n, sx, sy)) if *n == surface => {
                    self.pressed_at = (px as f32 - sx, py as f32 - sy);
                    (*sx, *sy)
                }
                _ if self.pressed.is_some() => {
                    (px as f32 - self.pressed_at.0, py as f32 - self.pressed_at.1)
                }
                _ => (-1.0, -1.0),
            };
            let kind = if motion {
                PointerKind::Move
            } else if press && button != 3 {
                // A press on a surface takes the keyboard from any other
                // one: Host::pointer does that.
                self.pressed = Some(surface.clone());
                PointerKind::Down
            } else {
                self.pressed = None;
                PointerKind::Up
            };
            if motion || primary {
                push_effects(host.pointer(&surface, kind, sx, sy, mods), to_program);
            }
            return;
        }

        // Not ours. A click outside every surface takes the keyboard back.
        if press && !motion && primary {
            for other in host.surface_names().map(str::to_string).collect::<Vec<_>>() {
                if host.is_focused(&other) {
                    push_effects(host.blur(&other), to_program);
                }
            }
        }
        self.forward_mouse(b, px, py, press, to_program);
    }

    /// Re-encodes a mouse event for the program, if it asked for this kind.
    fn forward_mouse(&self, b: u32, px: u32, py: u32, press: bool, out: &mut Vec<u8>) {
        let motion = b & 32 != 0;
        let buttons_down = b & 3 != 3;
        let wants = if motion {
            self.program.contains(&1003) || (self.program.contains(&1002) && buttons_down)
        } else {
            self.program.contains(&1000)
                || self.program.contains(&1002)
                || self.program.contains(&1003)
                || self.program.contains(&9)
        };
        if !wants {
            return;
        }
        let final_byte = if press { 'M' } else { 'm' };
        if self.program.contains(&1016) {
            let _ = std::io::Write::write_fmt(
                out,
                format_args!("\x1b[<{};{};{}{}", b, px + 1, py + 1, final_byte),
            );
            return;
        }
        let (col, row) = (px / self.cell_w + 1, py / self.cell_h + 1);
        if self.program.contains(&1006) {
            let _ =
                std::io::Write::write_fmt(out, format_args!("\x1b[<{b};{col};{row}{final_byte}"));
        } else if col < 224 && row < 224 {
            // Legacy X10 encoding: release is button 3.
            let cb = if press { b } else { (b & !3) | 3 };
            out.extend_from_slice(&[
                0x1b,
                b'[',
                b'M',
                (32 + cb) as u8,
                (32 + col) as u8,
                (32 + row) as u8,
            ]);
        }
    }
}

fn push_effects(effects: Vec<Effect>, out: &mut Vec<u8>) {
    for e in effects {
        if let Effect::Reply(b) = e {
            out.extend_from_slice(&b);
        }
    }
}

fn placeholder_id(cell: &vt100::Cell) -> Option<u32> {
    if !cell.contents().starts_with('\u{10EEEE}') {
        return None;
    }
    match cell.fgcolor() {
        vt100::Color::Rgb(r, g, b) => Some((r as u32) << 16 | (g as u32) << 8 | b as u32),
        vt100::Color::Idx(i) => Some(i as u32),
        vt100::Color::Default => None,
    }
}

fn diacritics(cell: &vt100::Cell) -> Option<(u16, u16)> {
    let mut chars = cell.contents().chars().skip(1);
    let idx = |c: char| DIACRITICS.iter().position(|&d| d == c).map(|p| p as u16);
    let row = chars.next().and_then(idx)?;
    let col = chars.next().and_then(idx).unwrap_or(0);
    Some((row, col))
}

enum Parsed {
    Incomplete,
    Mouse {
        len: usize,
        b: u32,
        x: u32,
        y: u32,
        press: bool,
    },
    Key {
        len: usize,
        key: Option<KeyIn>,
    },
}

/// A key as the terminal reported it.
struct KeyIn {
    key: Key,
    /// The key itself, unshifted: what pairs a release with its press.
    base: KeyName,
    /// A release, which the kitty keyboard protocol reports when the program
    /// asked for event types. A repeat is a press.
    release: bool,
}

impl KeyIn {
    fn press(name: KeyName, mods: Mods) -> KeyIn {
        KeyIn {
            base: unshifted(&name),
            key: Key { name, mods },
            release: false,
        }
    }
}

fn unshifted(name: &KeyName) -> KeyName {
    match name {
        KeyName::Char(s) => KeyName::Char(s.to_lowercase()),
        n => n.clone(),
    }
}

/// Parses one input item: an SGR mouse report, a key, or a byte.
fn parse_one(d: &[u8]) -> Parsed {
    let key = |len, name, mods| Parsed::Key {
        len,
        key: Some(KeyIn::press(name, mods)),
    };
    let none = Mods::default();
    let shift = Mods {
        shift: true,
        ..Mods::default()
    };
    match d[0] {
        0x1b => {
            if d.len() == 1 {
                // A lone ESC at the end of a read is the Escape key.
                return key(1, KeyName::Escape, none);
            }
            match d[1] {
                b'[' => parse_csi(d),
                b'O' if d.len() >= 3 => {
                    let name = match d[2] {
                        b'A' => KeyName::Up,
                        b'B' => KeyName::Down,
                        b'C' => KeyName::Right,
                        b'D' => KeyName::Left,
                        b'H' => KeyName::Home,
                        b'F' => KeyName::End,
                        _ => KeyName::Other,
                    };
                    key(3, name, none)
                }
                b'O' => Parsed::Incomplete,
                0x1b => key(1, KeyName::Escape, none),
                c if c >= 0x20 => {
                    // Alt + key.
                    let (len, ch) = utf8_char(&d[1..]);
                    match ch {
                        Some(s) => key(
                            1 + len,
                            KeyName::Char(s),
                            Mods {
                                alt: true,
                                ..Mods::default()
                            },
                        ),
                        None => Parsed::Incomplete,
                    }
                }
                _ => key(1, KeyName::Escape, none),
            }
        }
        b'\r' | b'\n' => key(1, KeyName::Enter, none),
        b'\t' => key(1, KeyName::Tab, none),
        0x7f | 0x08 => key(1, KeyName::Backspace, none),
        b' ' => key(1, KeyName::Space, none),
        c @ 0x01..=0x1a => key(
            1,
            KeyName::Char(((c - 1 + b'a') as char).to_string()),
            Mods {
                ctrl: true,
                ..Mods::default()
            },
        ),
        c if c < 0x20 => Parsed::Key { len: 1, key: None },
        _ => {
            let (len, ch) = utf8_char(d);
            match ch {
                Some(s) => {
                    let m = if s.chars().all(|c| c.is_uppercase()) {
                        shift
                    } else {
                        none
                    };
                    key(len, KeyName::Char(s), m)
                }
                None if len == 0 => Parsed::Incomplete,
                None => Parsed::Key { len, key: None },
            }
        }
    }
}

fn utf8_char(d: &[u8]) -> (usize, Option<String>) {
    let need = match d[0] {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => return (1, None),
    };
    if d.len() < need {
        return (0, None);
    }
    match std::str::from_utf8(&d[..need]) {
        Ok(s) => (need, Some(s.to_string())),
        Err(_) => (1, None),
    }
}

fn parse_csi(d: &[u8]) -> Parsed {
    // ESC [ params final
    let mut end = 2;
    while end < d.len() && !(0x40..=0x7e).contains(&d[end]) {
        end += 1;
        if end > 64 {
            return Parsed::Key {
                len: end,
                key: None,
            };
        }
    }
    if end >= d.len() {
        return Parsed::Incomplete;
    }
    let len = end + 1;
    let params = &d[2..end];
    let fin = d[end];
    if params.first() == Some(&b'<') && (fin == b'M' || fin == b'm') {
        let nums: Vec<u32> = params[1..]
            .split(|&c| c == b';')
            .map(|p| {
                std::str::from_utf8(p)
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0)
            })
            .collect();
        if let [b, x, y] = nums.as_slice() {
            return Parsed::Mouse {
                len,
                b: *b,
                x: *x,
                y: *y,
                press: fin == b'M',
            };
        }
        return Parsed::Key { len, key: None };
    }
    // Each parameter with its ':' fields, where the kitty keyboard protocol
    // puts the shifted key (code:shifted), the event type (mods:event) and
    // the text (codepoint:codepoint…). An empty field reads as 0.
    let fields: Vec<Vec<u32>> = params
        .split(|&c| c == b';')
        .map(|p| {
            p.split(|&c| c == b':')
                .map(|f| {
                    std::str::from_utf8(f)
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0)
                })
                .collect()
        })
        .collect();
    let field = |i: usize, j: usize| {
        fields
            .get(i)
            .and_then(|p| p.get(j))
            .copied()
            .unwrap_or(0)
    };
    // 1 + a mask: Shift 1, Alt 2, Ctrl 4, Super 8, Hyper 16, Meta 32,
    // Caps Lock 64, Num Lock 128.
    let bits = field(1, 0).saturating_sub(1);
    let mods = Mods {
        shift: bits & 1 != 0,
        alt: bits & 2 != 0,
        ctrl: bits & 4 != 0,
        meta: bits & 8 != 0,
    };
    // 1 a press, 2 a repeat, 3 a release.
    let release = field(1, 1) == 3;
    let code = field(0, 0);
    let mut base = None;
    let name = match fin {
        b'A' => KeyName::Up,
        b'B' => KeyName::Down,
        b'C' => KeyName::Right,
        b'D' => KeyName::Left,
        b'H' => KeyName::Home,
        b'F' => KeyName::End,
        b'Z' => {
            return Parsed::Key {
                len,
                key: Some(KeyIn::press(
                    KeyName::Tab,
                    Mods {
                        shift: true,
                        ..Mods::default()
                    },
                )),
            };
        }
        b'~' => match code {
            1 | 7 => KeyName::Home,
            4 | 8 => KeyName::End,
            3 => KeyName::Delete,
            5 => KeyName::PageUp,
            6 => KeyName::PageDown,
            _ => KeyName::Other,
        },
        // kitty keyboard protocol: CSI code ; mods u
        b'u' => match code {
            9 => KeyName::Tab,
            13 => KeyName::Enter,
            27 => KeyName::Escape,
            127 => KeyName::Backspace,
            32 => KeyName::Space,
            // kitty's functional keys are in the private use area. The
            // keypad's are the keys they stand for; the rest (modifiers,
            // locks, media, F13 and up) type nothing.
            57399..=57408 => KeyName::Char(char::from(b'0' + (code - 57399) as u8).to_string()),
            57409 => KeyName::Char(".".into()),
            57410 => KeyName::Char("/".into()),
            57411 => KeyName::Char("*".into()),
            57412 => KeyName::Char("-".into()),
            57413 => KeyName::Char("+".into()),
            57414 => KeyName::Enter,
            57415 => KeyName::Char("=".into()),
            57417 => KeyName::Left,
            57418 => KeyName::Right,
            57419 => KeyName::Up,
            57420 => KeyName::Down,
            57421 => KeyName::PageUp,
            57422 => KeyName::PageDown,
            57423 => KeyName::Home,
            57424 => KeyName::End,
            57426 => KeyName::Delete,
            0xe000..=0xf8ff => KeyName::Other,
            c if c > 32 => match char::from_u32(c) {
                Some(ch) => {
                    base = Some(KeyName::Char(ch.to_string()));
                    KeyName::Char(kitty_text(&fields, ch, mods.shift, bits & 64 != 0))
                }
                None => KeyName::Other,
            },
            _ => KeyName::Other,
        },
        _ => KeyName::Other,
    };
    Parsed::Key {
        len,
        key: Some(KeyIn {
            base: base.unwrap_or_else(|| unshifted(&name)),
            key: Key { name, mods },
            release,
        }),
    }
}

/// The text a kitty key types: the text it reports (flag 16), or else its
/// shifted key (flag 4) while Shift is held, or else the key itself, in
/// upper case with Shift or Caps Lock.
fn kitty_text(fields: &[Vec<u32>], key: char, shift: bool, caps: bool) -> String {
    let text: String = fields
        .get(2)
        .into_iter()
        .flatten()
        .filter(|&&c| c != 0)
        .filter_map(|&c| char::from_u32(c))
        .collect();
    if !text.is_empty() {
        return text;
    }
    let shifted = fields[0].get(1).copied().filter(|&c| c != 0);
    match shifted.and_then(char::from_u32) {
        Some(s) if shift => s.to_string(),
        _ if shift || caps => key.to_uppercase().collect(),
        _ => key.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The key in `d`, which must be one whole key: its name, unshifted
    /// name, Shift, and whether it is a release.
    fn key(d: &str) -> (KeyName, KeyName, bool, bool) {
        match parse_one(d.as_bytes()) {
            Parsed::Key { len, key: Some(k) } if len == d.len() => {
                (k.key.name, k.base, k.key.mods.shift, k.release)
            }
            _ => panic!("{d:?} is not one key"),
        }
    }

    fn ch(s: &str) -> KeyName {
        KeyName::Char(s.into())
    }

    #[test]
    fn kitty_event_types() {
        assert_eq!(key("\x1b[97u"), (ch("a"), ch("a"), false, false));
        assert_eq!(key("\x1b[97;1:2u"), (ch("a"), ch("a"), false, false));
        assert_eq!(key("\x1b[97;1:3u"), (ch("a"), ch("a"), false, true));
        use KeyName::*;
        assert_eq!(key("\x1b[127;1:3u"), (Backspace, Backspace, false, true));
        assert_eq!(key("\x1b[3;1:3~"), (Delete, Delete, false, true));
        assert_eq!(key("\x1b[1;1:3D"), (Left, Left, false, true));
        assert_eq!(key("\x1b[1;2:3H"), (Home, Home, true, true));
    }

    #[test]
    fn kitty_shifted_keys_and_text() {
        // Alternate keys (flag 4): the shifted key is the second field.
        assert_eq!(key("\x1b[97:65;2u"), (ch("A"), ch("a"), true, false));
        assert_eq!(key("\x1b[50:64;2u"), (ch("@"), ch("2"), true, false));
        // The release after Shift came up still pairs with the press.
        assert_eq!(key("\x1b[50;1:3u").1, ch("2"));
        // Without it, Shift upper-cases; so does Caps Lock.
        assert_eq!(key("\x1b[97;2u").0, ch("A"));
        assert_eq!(key("\x1b[97;65u"), (ch("A"), ch("a"), false, false));
        // Associated text (flag 16) wins.
        assert_eq!(key("\x1b[97;2;65u").0, ch("A"));
        assert_eq!(key("\x1b[50:64;2;64u").0, ch("@"));
    }

    #[test]
    fn kitty_functional_keys_type_nothing_but_the_keypad() {
        use KeyName::*;
        // Left Shift, Right Ctrl, Caps Lock, F13, Media Play.
        for code in [57441, 57448, 57358, 57376, 57428] {
            assert_eq!(key(&format!("\x1b[{code}u")).0, Other, "{code}");
            assert_eq!(key(&format!("\x1b[{code};1:3u")).0, Other, "{code}");
        }
        assert_eq!(key("\x1b[57399u").0, ch("0"));
        assert_eq!(key("\x1b[57408u").0, ch("9"));
        assert_eq!(key("\x1b[57409u").0, ch("."));
        assert_eq!(key("\x1b[57414u").0, Enter);
        assert_eq!(key("\x1b[57417u").0, Left);
        assert_eq!(key("\x1b[57426u").0, Delete);
    }

    #[test]
    fn legacy_keys_are_presses() {
        use KeyName::*;
        assert_eq!(key("\x7f"), (Backspace, Backspace, false, false));
        assert_eq!(key("A"), (ch("A"), ch("a"), true, false));
        assert_eq!(key("\x1b[D"), (Left, Left, false, false));
        assert_eq!(key("\x1b[1;5D").0, Left);
        assert_eq!(key("\x1b[Z"), (Tab, Tab, true, false));
    }
}

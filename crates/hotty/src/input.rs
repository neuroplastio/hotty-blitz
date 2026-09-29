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
//! already in the encoding the program asked the terminal for.

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

    fn key(&mut self, raw: &[u8], key: Option<Key>, host: &mut Host, to_program: &mut Vec<u8>) {
        let focused = host.focused_surface().map(str::to_string);
        if let (Some(surface), Some(key)) = (focused, key) {
            let outcome = host.key(&surface, &key);
            push_effects(outcome.effects, to_program);
            if outcome.consumed {
                return;
            }
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
        let target = self.hit(px, py).filter(|_| !wheel);
        if !motion {
            crate::log(&format!(
                "mouse b={b} px=({px},{py}) press={press} hit={target:?}"
            ));
        }
        let over = target.as_ref().map(|t| t.0.clone());

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
            let (sx, sy) = match &target {
                Some((n, sx, sy)) if *n == surface => (*sx, *sy),
                _ => (-1.0, -1.0),
            };
            let kind = if motion {
                PointerKind::Move
            } else if press && button != 3 {
                self.pressed = Some(surface.clone());
                // A press on a surface takes the keyboard from any other one.
                for other in host.surface_names().map(str::to_string).collect::<Vec<_>>() {
                    if other != surface && host.is_focused(&other) {
                        push_effects(host.blur(&other), to_program);
                    }
                }
                PointerKind::Down
            } else {
                self.pressed = None;
                PointerKind::Up
            };
            if button == 0 || motion || kind == PointerKind::Up {
                push_effects(host.pointer(&surface, kind, sx, sy, mods), to_program);
            }
            return;
        }

        // Not ours. A press outside every surface takes the keyboard back.
        if press && !motion && !wheel {
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
        key: Option<Key>,
    },
}

/// Parses one input item: an SGR mouse report, a key, or a byte.
fn parse_one(d: &[u8]) -> Parsed {
    let key = |len, name, mods| Parsed::Key {
        len,
        key: Some(Key { name, mods }),
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
    let nums: Vec<u32> = params
        .split(|&c| c == b';')
        .map(|p| {
            std::str::from_utf8(p.split(|&c| c == b':').next().unwrap_or(p))
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
        })
        .collect();
    let mods = match nums.get(1) {
        Some(&m) if m > 1 => {
            let m = m - 1;
            Mods {
                shift: m & 1 != 0,
                alt: m & 2 != 0,
                ctrl: m & 4 != 0,
                meta: m & 8 != 0,
            }
        }
        _ => Mods::default(),
    };
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
                key: Some(Key {
                    name: KeyName::Tab,
                    mods: Mods {
                        shift: true,
                        ..Mods::default()
                    },
                }),
            };
        }
        b'~' => match nums.first() {
            Some(1) | Some(7) => KeyName::Home,
            Some(4) | Some(8) => KeyName::End,
            Some(3) => KeyName::Delete,
            Some(5) => KeyName::PageUp,
            Some(6) => KeyName::PageDown,
            _ => KeyName::Other,
        },
        b'u' => {
            // kitty keyboard protocol: CSI code ; mods u
            match nums.first().copied().unwrap_or(0) {
                9 => KeyName::Tab,
                13 => KeyName::Enter,
                27 => KeyName::Escape,
                127 => KeyName::Backspace,
                32 => KeyName::Space,
                c if c >= 32 => match char::from_u32(c) {
                    Some(ch) => KeyName::Char(if mods.shift {
                        ch.to_uppercase().collect()
                    } else {
                        ch.to_string()
                    }),
                    None => KeyName::Other,
                },
                _ => KeyName::Other,
            }
        }
        _ => KeyName::Other,
    };
    Parsed::Key {
        len,
        key: Some(Key { name, mods }),
    }
}

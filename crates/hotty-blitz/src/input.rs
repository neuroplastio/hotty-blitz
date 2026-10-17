//! Input from the host and events for the program (SPEC §9, §10).
//!
//! The host hands pointer positions and keys to a surface. The surface's
//! document runs HTML's default actions locally (hover, focus, typing into
//! inputs, toggling checkboxes and `<details>`), and what the program must
//! hear about comes back as [`Event`]s encoded for its stdin.

use crate::Effect;
use crate::body::{self, Detail};
use hotty_wire::{Control, keys};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Move,
    Down,
    Up,
    /// Pointer left the surface.
    Leave,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_: bool,
}

impl Mods {
    /// The modifier keys held as an event's `mods` carries them (SPEC §9):
    /// 1 shift, 2 alt, 4 ctrl, 8 super; `None` when none is held, as the
    /// field is then left out.
    pub fn bits(&self) -> Option<u8> {
        let bits = u8::from(self.shift)
            | u8::from(self.alt) << 1
            | u8::from(self.ctrl) << 2
            | u8::from(self.super_) << 3;
        (bits != 0).then_some(bits)
    }
}

/// A key, decoded by the host from whatever the terminal sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyName {
    /// Text to insert (a printable key, already shifted).
    Char(String),
    Enter,
    Tab,
    Backspace,
    Delete,
    Escape,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Space,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    pub name: KeyName,
    pub mods: Mods,
    /// The names the press has (SPEC §10.4), canonical, the physical one
    /// first; empty for the one name [`Key::spec_name`] gives.
    pub names: Vec<String>,
}

impl Key {
    /// A key with the one name its [`KeyName`] and modifiers give.
    pub fn new(name: KeyName, mods: Mods) -> Key {
        Key {
            name,
            mods,
            names: Vec::new(),
        }
    }

    /// The key as hotty SPEC §10.4 names it, the name a keymap looks up
    /// (`ctrl+a`, `alt+arrow_left`, `shift+a`): a character, or the physical
    /// key where it has none. `None` for a key with no name here.
    pub fn spec_name(&self) -> Option<String> {
        let m = self.mods;
        let bits = keys::KeyParts::new(m.super_, m.ctrl, m.alt, m.shift, "").mods;
        let value = match &self.name {
            KeyName::Char(s) => return Some(keys::char_key(bits, s)),
            KeyName::Space => "space",
            KeyName::Enter => "enter",
            KeyName::Tab => "tab",
            KeyName::Backspace => "backspace",
            KeyName::Delete => "delete",
            KeyName::Escape => "escape",
            KeyName::Left => "arrow_left",
            KeyName::Right => "arrow_right",
            KeyName::Up => "arrow_up",
            KeyName::Down => "arrow_down",
            KeyName::Home => "home",
            KeyName::End => "end",
            KeyName::PageUp => "page_up",
            KeyName::PageDown => "page_down",
            KeyName::Other => return None,
        };
        Some(keys::KeyParts { mods: bits, value: value.into() }.name())
    }

    /// The names a keymap looks up for the press (SPEC §10.4), the physical
    /// one first: those it has, else the one [`Key::spec_name`] gives.
    pub fn names(&self) -> Vec<String> {
        if self.names.is_empty() {
            self.spec_name().into_iter().collect()
        } else {
            self.names.clone()
        }
    }

    /// The key a name of hotty SPEC §10.4 names (as [`keys::decode_keys`]
    /// reads it from what the terminal sent); `None` for a name that does
    /// not parse.
    pub fn from_spec_name(name: &str) -> Option<Key> {
        Key::pressed(name, "")
    }

    /// The key a press is, as the user pressed it (SPEC §10.4): its physical
    /// name (`ctrl+key_w`) and the other, the character it types (`ctrl+w`)
    /// or, for a modifier tapped alone, its modifier key (`shift`), each
    /// possibly empty. `None` when neither parses.
    pub fn pressed(physical: &str, other: &str) -> Option<Key> {
        let (p, o) = (keys::parse_key(physical), keys::parse_key(other));
        let mut names: Vec<String> = Vec::new();
        for n in [&p, &o].into_iter().flatten() {
            if !names.contains(n) {
                names.push(n.clone());
            }
        }
        // The key is what the character is, else what the place is.
        let parts = |n: &Option<String>| n.as_deref().map(keys::KeyParts::split);
        let k = parts(&o)
            .filter(|k| k.kind() != keys::Kind::Modifier)
            .or_else(|| parts(&p))
            .or_else(|| parts(&o))?;
        let name = match (k.kind(), k.value.as_str()) {
            (keys::Kind::Modifier, _) => KeyName::Other,
            (_, "space") => KeyName::Space,
            (_, "enter" | "numpad_enter") => KeyName::Enter,
            (_, "tab") => KeyName::Tab,
            (_, "backspace") => KeyName::Backspace,
            (_, "delete") => KeyName::Delete,
            (_, "escape") => KeyName::Escape,
            (_, "arrow_left") => KeyName::Left,
            (_, "arrow_right") => KeyName::Right,
            (_, "arrow_up") => KeyName::Up,
            (_, "arrow_down") => KeyName::Down,
            (_, "home") => KeyName::Home,
            (_, "end") => KeyName::End,
            (_, "page_up") => KeyName::PageUp,
            (_, "page_down") => KeyName::PageDown,
            (keys::Kind::Character, v) => {
                KeyName::Char(keys::key_text(&k.name()).unwrap_or_else(|| v.to_string()))
            }
            _ => KeyName::Other,
        };
        let mut key = Key::new(
            name,
            Mods {
                shift: k.shift(),
                ctrl: k.ctrl(),
                alt: k.alt(),
                super_: k.super_(),
            },
        );
        // The one name a key has is what `spec_name` gives.
        if key.names() != names {
            key.names = names;
        }
        Some(key)
    }
}

#[derive(Debug, Default)]
pub struct KeyOutcome {
    /// The focused element used the key. If false, forward it to the program.
    pub consumed: bool,
    pub effects: Vec<Effect>,
}

/// What became of a wheel ([`crate::Host::wheel`]).
#[derive(Debug, Default)]
pub struct WheelOutcome {
    /// The surface took it. If false, the host handles it as over the cells.
    pub taken: bool,
    pub effects: Vec<Effect>,
}

/// A finger's phase, as the terminal reports a touch ([`crate::Host::touch`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    /// The first finger touched: a touch begins.
    Down,
    /// It moved.
    Move,
    /// It lifted.
    Up,
    /// A second finger touched, or the platform cancelled the touch: the
    /// rest of the gesture is the terminal's, until the next `Down`.
    Cancel,
    /// The terminal took the touch for a long press: it never drags.
    LongPress,
}

/// Whose a touch is, so far ([`crate::Host::touch`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Touch {
    /// The terminal's: it scrolls with the touch, or taps or long-presses
    /// with it, as with any touch.
    #[default]
    Terminal,
    /// Not decided yet: it may still drag. The terminal holds it, neither
    /// scrolling with it nor taking it for a long press's moves.
    Undecided,
    /// The surface's: it drags (SPEC §9.1). The terminal does nothing with
    /// it.
    Surface,
}

/// What became of a touch's phase ([`crate::Host::touch`]).
#[derive(Debug, Default)]
pub struct TouchOutcome {
    pub touch: Touch,
    pub effects: Vec<Effect>,
}

/// Whose a touch on the cells is, so far ([`crate::Host::cells_touch`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CellsTouch {
    /// The terminal's: it scrolls with the touch, or taps or long-presses
    /// with it, as with any touch.
    #[default]
    Terminal,
    /// It began in an area of the program's (SPEC §9.6) and may still
    /// drag: the terminal holds it, neither scrolling with it nor taking
    /// it for a tap.
    Undecided,
    /// The program's mouse (SPEC §9.6): the terminal makes the reports
    /// that come with it, and does nothing else with the touch.
    Mouse,
}

/// A report of the mouse's primary button that the terminal makes for a
/// finger in the cells (SPEC §9.6), as it reports the mouse's: in the
/// program's mode and encoding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MouseReport {
    /// `Down`, `Move` (with the button held) or `Up`.
    pub kind: PointerKind,
    /// Where, in device pixels from the top left of the screen's cells, as
    /// the touch was given: for the press, where the touch began.
    pub x: f32,
    pub y: f32,
    /// The cell that is, counted from 0 at the screen's top left: negative,
    /// or past the screen, where the finger is off it.
    pub c: i64,
    pub r: i64,
    /// The keys held, as the touch was given: the terminal leaves out one
    /// with which it keeps a mouse press for itself, such as Shift where
    /// Shift and a press select (SPEC §9.6).
    pub mods: Mods,
}

/// What became of a phase of a touch on the cells
/// ([`crate::Host::cells_touch`]).
#[derive(Debug, Default)]
pub struct CellsTouchOutcome {
    pub touch: CellsTouch,
    /// The reports to make of the mouse, in order.
    pub mouse: Vec<MouseReport>,
    /// Events for the program: a press on the cells gives the keyboard
    /// back (SPEC §10.1), and a touch on a surface that never lifted ends
    /// its drag when this one begins.
    pub effects: Vec<Effect>,
}

/// Something the program should hear about.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub kind: &'static str,
    /// The id of the element it names; empty where none is meant.
    pub target: String,
    /// Its body, a msgpack map (SPEC §9); none for a kind that has none.
    pub detail: Option<Detail>,
}

impl Event {
    pub fn encode(&self, surface: &str) -> Vec<u8> {
        let mut c = Control::default();
        c.set("a", "ev");
        c.set("s", surface);
        c.set("e", self.kind);
        // `t` names an element, and is absent where none is meant (SPEC §9).
        if !self.target.is_empty() {
            c.set("t", self.target.as_str());
        }
        let body = self.detail.as_ref().map(body::encode).unwrap_or_default();
        hotty_wire::encode_plain(&c, &body)
    }
}

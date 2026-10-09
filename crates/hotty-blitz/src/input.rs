//! Input from the host and events for the program (SPEC §9, §10).
//!
//! The host hands pointer positions and keys to a surface. The surface's
//! document runs HTML's default actions locally (hover, focus, typing into
//! inputs, toggling checkboxes and `<details>`), and what the program must
//! hear about comes back as [`Event`]s encoded for its stdin.

use crate::Effect;
use hotty_wire::{Control, keys};

/// Event types a host can report, for the capability reply.
/// `drag` stands for `dragstart`, `drag` and `dragend` (SPEC §4, §9.1).
pub const EVENTS: &[&str] = &[
    "click", "change", "input", "submit", "press", "drag", "focus", "blur", "fit", "hover",
];

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
    pub meta: bool,
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
}

impl Key {
    /// The key as HOTTY SPEC §10.4 names it, the name a text field's keymap
    /// looks up (`Control+a`, `Alt+ArrowLeft`, `A`); `None` for a key with
    /// no name here.
    pub fn spec_name(&self) -> Option<String> {
        let value = match &self.name {
            KeyName::Char(s) => s.as_str(),
            KeyName::Space => " ",
            KeyName::Enter => "Enter",
            KeyName::Tab => "Tab",
            KeyName::Backspace => "Backspace",
            KeyName::Delete => "Delete",
            KeyName::Escape => "Escape",
            KeyName::Left => "ArrowLeft",
            KeyName::Right => "ArrowRight",
            KeyName::Up => "ArrowUp",
            KeyName::Down => "ArrowDown",
            KeyName::Home => "Home",
            KeyName::End => "End",
            KeyName::PageUp => "PageUp",
            KeyName::PageDown => "PageDown",
            KeyName::Other => return None,
        };
        let m = self.mods;
        let k = keys::KeyParts::new(m.ctrl, m.alt, m.meta, m.shift, value);
        keys::parse_key(&k.name())
    }

    /// The key a name of HOTTY SPEC §10.4 names (as [`keys::decode_keys`]
    /// reads it from what the terminal sent); `None` for a key with no
    /// [`KeyName`].
    pub fn from_spec_name(name: &str) -> Option<Key> {
        let k = keys::split_key(name)?;
        let name = match k.value.as_str() {
            " " => KeyName::Space,
            "Enter" => KeyName::Enter,
            "Tab" => KeyName::Tab,
            "Backspace" => KeyName::Backspace,
            "Delete" => KeyName::Delete,
            "Escape" => KeyName::Escape,
            "ArrowLeft" => KeyName::Left,
            "ArrowRight" => KeyName::Right,
            "ArrowUp" => KeyName::Up,
            "ArrowDown" => KeyName::Down,
            "Home" => KeyName::Home,
            "End" => KeyName::End,
            "PageUp" => KeyName::PageUp,
            "PageDown" => KeyName::PageDown,
            v if keys::is_char(v) => KeyName::Char(v.to_string()),
            _ => KeyName::Other,
        };
        Some(Key {
            name,
            mods: Mods {
                shift: k.shift(),
                ctrl: k.control(),
                alt: k.alt(),
                meta: k.meta(),
            },
        })
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

/// Something the program should hear about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub kind: &'static str,
    pub target: String,
    pub detail: serde_json::Value,
}

impl Event {
    pub fn encode(&self, surface: &str) -> Vec<u8> {
        let mut c = Control::default();
        c.set("a", "ev");
        c.set("s", surface);
        c.set("e", self.kind);
        c.set("t", self.target.as_str());
        let body = if self.detail.is_null() {
            Vec::new()
        } else {
            self.detail.to_string().into_bytes()
        };
        hotty_wire::encode_plain(&c, &body)
    }
}

//! Input from the host and events for the program (SPEC §9, §10).
//!
//! The host hands pointer positions and keys to a surface. The surface's
//! document runs HTML's default actions locally (hover, focus, typing into
//! inputs, toggling checkboxes and `<details>`), and what the program must
//! hear about comes back as [`Event`]s encoded for its stdin.

use crate::Effect;
use hotty_wire::Control;

/// Event types a host can report, for the capability reply.
/// `drag` stands for `dragstart`, `drag` and `dragend` (SPEC §4, §9.1).
pub const EVENTS: &[&str] = &[
    "click", "change", "input", "submit", "press", "drag", "focus", "blur",
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

#[derive(Debug, Default)]
pub struct KeyOutcome {
    /// The focused element used the key. If false, forward it to the program.
    pub consumed: bool,
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
        c.set("t", sanitize(&self.target));
        let body = if self.detail.is_null() {
            Vec::new()
        } else {
            self.detail.to_string().into_bytes()
        };
        hotty_wire::encode(&c, &body)
    }
}

/// Control values cannot hold `:`, `;` or `=`; ids that do are reported mangled.
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c == ':' || c == ';' || c == '=' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

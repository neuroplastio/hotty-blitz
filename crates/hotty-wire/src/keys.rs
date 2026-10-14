//! Key names and keymaps (HOTTY SPEC §10.2, §10.4).
//!
//! A host names a key as the program would read it: [`decode_keys`] reads
//! what the terminal would send the program, and [`parse_key`] reads a name
//! a document writes. A text field's keymap is the default keymap, then the
//! `data-keys` of each element from the root to the field ([`resolve`]);
//! [`Keymap::lookup`] says what the field does with a key. Any other
//! focused element's keymap ([`element_keymap`]) gives keys to the program
//! ([`Keymap::program`]) or scrolls with them ([`Keymap::scroll`]). A
//! keymap reads a binding's `Mod` as its [`Platform`]'s (SPEC §10.4).

use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// The actions a keymap binds (SPEC §10.2).
pub const ACTIONS: &[&str] = &[
    "char-backward",
    "char-forward",
    "word-backward",
    "word-forward",
    "line-start",
    "line-end",
    "delete-char-backward",
    "delete-char-forward",
    "delete-word-backward",
    "delete-word-forward",
    "delete-to-line-start",
    "delete-to-line-end",
    "line-previous",
    "line-next",
    "page-up",
    "page-down",
    "input-start",
    "input-end",
    "select-all",
    "newline",
    "submit",
    "program",
    "copy",
    "cut",
    "paste",
    "undo",
    "redo",
    "scroll-up",
    "scroll-down",
    "scroll-left",
    "scroll-right",
    "scroll-page-up",
    "scroll-page-down",
    "scroll-half-page-up",
    "scroll-half-page-down",
    "scroll-start",
    "scroll-end",
];

/// Whether an action is a scroll action, which a text field's keymap leaves
/// out (SPEC §10.2, scrolling keys).
pub fn scroll_action(action: &str) -> bool {
    action.starts_with("scroll-") && ACTIONS.contains(&action)
}

/// What [`Keymap::lookup`] returns for a character the field types.
pub const INSERT: &str = "insert";

/// Whether only a multi-line field (a textarea) has the action. From an
/// input, a key bound to one reaches the program.
pub fn multiline_action(action: &str) -> bool {
    matches!(
        action,
        "line-previous" | "line-next" | "page-up" | "page-down" | "newline"
    )
}

/// Whether an action is a move, which selects with Shift (SPEC §10.2).
pub fn move_action(action: &str) -> bool {
    matches!(
        action,
        "char-backward"
            | "char-forward"
            | "word-backward"
            | "word-forward"
            | "line-start"
            | "line-end"
            | "line-previous"
            | "line-next"
            | "page-up"
            | "page-down"
            | "input-start"
            | "input-end"
    )
}

/// The SDK's keymap (SDK.md §3.10), bubbles' text input and text area, but
/// Mod+a selects all, as a `data-keys` value.
pub const TERMINAL_KEYS: &str = concat!(
    "ArrowLeft=char-backward Control+b=char-backward ArrowRight=char-forward Control+f=char-forward ",
    "Alt+ArrowLeft=word-backward Control+ArrowLeft=word-backward Alt+b=word-backward ",
    "Alt+ArrowRight=word-forward Control+ArrowRight=word-forward Alt+f=word-forward ",
    "Home=line-start End=line-end Control+e=line-end ",
    "Backspace=delete-char-backward Control+h=delete-char-backward ",
    "Delete=delete-char-forward Control+d=delete-char-forward ",
    "Alt+Backspace=delete-word-backward Control+w=delete-word-backward Control+Backspace=delete-word-backward ",
    "Alt+Delete=delete-word-forward Alt+d=delete-word-forward Control+Delete=delete-word-forward ",
    "Control+u=delete-to-line-start Control+k=delete-to-line-end ",
    "ArrowUp=line-previous Control+p=line-previous ArrowDown=line-next Control+n=line-next ",
    "PageUp=page-up PageDown=page-down ",
    "Alt+<=input-start Control+Home=input-start Alt+>=input-end Control+End=input-end ",
    "Mod+a=select-all Control+m=newline",
);

/// The modifiers, in the order a key's name writes them. `Mod` is a
/// binding's platform modifier (SPEC §10.4), which a keymap reads as its
/// [`Platform`]'s; a pressed key is never named with it.
const MODIFIERS: [&str; 5] = ["Mod", "Control", "Alt", "Meta", "Shift"];
const MOD: u8 = 1;
const CONTROL: u8 = 2;
const ALT: u8 = 4;
const META: u8 = 8;
const SHIFT: u8 = 16;

/// A key's name, split: its modifiers (a bit for each of [`MODIFIERS`]) and
/// its value, `" "` for the space bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyParts {
    pub mods: u8,
    pub value: String,
}

impl KeyParts {
    pub fn control(&self) -> bool {
        self.mods & CONTROL != 0
    }
    pub fn alt(&self) -> bool {
        self.mods & ALT != 0
    }
    pub fn meta(&self) -> bool {
        self.mods & META != 0
    }
    pub fn shift(&self) -> bool {
        self.mods & SHIFT != 0
    }

    /// Builds a key from its modifiers and its value.
    pub fn new(control: bool, alt: bool, meta: bool, shift: bool, value: &str) -> KeyParts {
        let mut mods = 0;
        for (on, bit) in [(control, CONTROL), (alt, ALT), (meta, META), (shift, SHIFT)] {
            if on {
                mods |= bit;
            }
        }
        KeyParts {
            mods,
            value: value.to_string(),
        }
    }

    /// The key with its `Mod` read as `meta` says: Meta, or Control.
    fn read_mod(mut self, meta: bool) -> KeyParts {
        if self.mods & MOD != 0 {
            self.mods = self.mods & !MOD | if meta { META } else { CONTROL };
        }
        self
    }

    /// Shift shown in a letter where it can be: Shift with a small letter is
    /// its capital, and Shift with a capital is the capital.
    fn canonical(mut self) -> KeyParts {
        if !self.shift() || !is_char(&self.value) {
            return self;
        }
        let mut cs = self.value.chars();
        let (Some(c), None) = (cs.next(), cs.next()) else {
            return self;
        };
        let up = single(c.to_uppercase());
        let low = single(c.to_lowercase());
        if let Some(u) = up.filter(|&u| u != c) {
            self.value = u.to_string();
            self.mods &= !SHIFT;
        } else if low.is_some_and(|l| l != c) {
            self.mods &= !SHIFT;
        }
        self
    }

    /// The key's canonical name (SPEC §10.4).
    pub fn name(&self) -> String {
        let mut out = String::new();
        for (i, m) in MODIFIERS.iter().enumerate() {
            if self.mods & (1 << i) != 0 {
                out.push_str(m);
                out.push('+');
            }
        }
        out.push_str(if self.value == " " {
            "Space"
        } else {
            &self.value
        });
        out
    }
}

/// The one char a case mapping gives, if it gives one.
fn single(mut it: impl Iterator<Item = char>) -> Option<char> {
    let c = it.next()?;
    it.next().is_none().then_some(c)
}

/// Whether a key's value is a character: one grapheme cluster that is not a
/// control.
pub fn is_char(v: &str) -> bool {
    let mut g = v.graphemes(true);
    match (g.next(), g.next()) {
        (Some(first), None) => !first.chars().next().is_some_and(char::is_control),
        _ => false,
    }
}

/// Whether a key's value is a named key value: a capital, then letters and
/// digits (`Enter`, `ArrowLeft`, `F12`).
fn is_named(v: &str) -> bool {
    let b = v.as_bytes();
    b.len() >= 2 && b[0].is_ascii_uppercase() && b.iter().all(u8::is_ascii_alphanumeric)
}

/// Reads a key's name, its modifiers in any order.
pub fn split_key(name: &str) -> Option<KeyParts> {
    let (head, value) = if name == "+" {
        ("", "+")
    } else if name.len() > 2 && name.ends_with("++") {
        (&name[..name.len() - 2], "+")
    } else {
        match name.rfind('+') {
            Some(i) => (&name[..i], &name[i + 1..]),
            None => ("", name),
        }
    };
    let mut mods = 0u8;
    if !head.is_empty() {
        for m in head.split('+') {
            let i = MODIFIERS.iter().position(|k| *k == m)?;
            if mods & (1 << i) != 0 {
                return None;
            }
            mods |= 1 << i;
        }
    }
    let value = if value == "Space" { " " } else { value };
    if !is_char(value) && !is_named(value) {
        return None;
    }
    Some(KeyParts {
        mods,
        value: value.to_string(),
    })
}

/// A key's name in its canonical form (SPEC §10.4): `Shift+Control+a` is
/// `Control+A`, `Control+ ` is `Control+Space`, and a binding's `Mod` is
/// kept, written first. `None` when it does not parse.
pub fn parse_key(name: &str) -> Option<String> {
    split_key(name).map(|k| k.canonical().name())
}

/// The keys in input from the terminal, as SPEC §10.4 names them: a
/// canonical name for each, `None` for input that is no key it names (a
/// mouse report, a sequence it does not know, a release). An ESC at the end
/// of the input is Escape.
pub fn decode_keys(input: &[u8]) -> Vec<Option<String>> {
    decode_key_spans(input).into_iter().map(|(_, k)| k).collect()
}

/// [`decode_keys`], with the bytes each key took: a host that uses some of
/// the keys sends the program the others as they came.
pub fn decode_key_spans(input: &[u8]) -> Vec<(Range<usize>, Option<String>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < input.len() {
        let at = i;
        let key = if input[i] != 0x1b {
            let (k, n) = one_key(&input[i..]);
            i += n;
            k
        } else if i + 1 == input.len() {
            i += 1;
            Some("Escape".into())
        } else if input[i + 1] == b'O' && i + 2 < input.len() {
            i += 3;
            final_key(input[i - 1]).map(str::to_string)
        } else if input[i + 1] == b'[' && i + 2 < input.len() {
            let mut j = i + 2;
            while j < input.len() && (0x30..=0x3f).contains(&input[j]) {
                j += 1;
            }
            let params = std::str::from_utf8(&input[i + 2..j]).unwrap_or("");
            while j < input.len() && (0x20..=0x2f).contains(&input[j]) {
                j += 1;
            }
            i = (j + 1).min(input.len());
            if j == input.len() || !(0x40..=0x7e).contains(&input[j]) {
                None
            } else {
                csi_key(params, input[j])
            }
        } else {
            let (k, n) = one_key(&input[i + 1..]);
            i += 1 + n;
            k.and_then(|k| {
                let mut p = split_key(&k)?;
                p.mods |= ALT;
                Some(p.canonical().name())
            })
        };
        out.push((at..i, key));
    }
    out
}

/// The key of the control or the character the input starts with, and its
/// length in bytes.
fn one_key(input: &[u8]) -> (Option<String>, usize) {
    let b = input[0];
    let name = match b {
        0x00 => Some("Control+Space"),
        0x08 => Some("Control+h"),
        0x09 => Some("Tab"),
        0x0d => Some("Enter"),
        0x1b => Some("Escape"),
        0x7f => Some("Backspace"),
        0x1c => Some("Control+\\"),
        0x1d => Some("Control+]"),
        0x1e => Some("Control+^"),
        0x1f => Some("Control+_"),
        0x20 => Some("Space"),
        _ => None,
    };
    if let Some(n) = name {
        return (Some(n.into()), 1);
    }
    if b < 0x20 {
        return (Some(format!("Control+{}", (b + 0x60) as char)), 1);
    }
    let end = input.len().min(64);
    let text = match std::str::from_utf8(&input[..end]) {
        Ok(t) => t,
        Err(e) if e.valid_up_to() > 0 => std::str::from_utf8(&input[..e.valid_up_to()]).unwrap(),
        Err(_) => return (None, 1),
    };
    let g = text.graphemes(true).next().unwrap_or("");
    // A cluster stops at a control, which is a key of its own.
    let g = match g.char_indices().skip(1).find(|(_, c)| c.is_control()) {
        Some((i, _)) => &g[..i],
        None => g,
    };
    (Some(g.to_string()), g.len().max(1))
}

/// The keys CSI and SS3 name by their final byte.
fn final_key(f: u8) -> Option<&'static str> {
    Some(match f {
        b'A' => "ArrowUp",
        b'B' => "ArrowDown",
        b'C' => "ArrowRight",
        b'D' => "ArrowLeft",
        b'H' => "Home",
        b'F' => "End",
        _ => return None,
    })
}

/// A CSI parameter `m[:e]`: its modifiers, and whether it is a release.
fn mods_of(field: &str) -> Option<(u8, bool)> {
    let (num, ev) = field.split_once(':').unwrap_or((field, ""));
    let n: i64 = if num.is_empty() { 1 } else { num.parse().ok()? };
    let e: i64 = if ev.is_empty() { 1 } else { ev.parse().ok()? };
    let bits = (n - 1).max(0);
    let mut m = 0;
    if bits & 1 != 0 {
        m |= SHIFT;
    }
    if bits & 2 != 0 {
        m |= ALT;
    }
    if bits & 4 != 0 {
        m |= CONTROL;
    }
    if bits & (8 | 32) != 0 {
        m |= META;
    }
    Some((m, e == 3))
}

/// The key a code of the kitty protocol or of modifyOtherKeys names.
fn code_key(code: u32, mut mods: u8, shifted: u32, text: &str) -> Option<String> {
    let named = match code {
        9 => Some("Tab"),
        13 => Some("Enter"),
        27 => Some("Escape"),
        8 | 127 => Some("Backspace"),
        57399..=57408 => {
            let value = char::from(b'0' + (code - 57399) as u8).to_string();
            return Some(KeyParts { mods, value }.name());
        }
        57409 => Some("."),
        57410 => Some("/"),
        57411 => Some("*"),
        57412 => Some("-"),
        57413 => Some("+"),
        57414 => Some("Enter"),
        57415 => Some("="),
        57417 => Some("ArrowLeft"),
        57418 => Some("ArrowRight"),
        57419 => Some("ArrowUp"),
        57420 => Some("ArrowDown"),
        57421 => Some("PageUp"),
        57422 => Some("PageDown"),
        57423 => Some("Home"),
        57424 => Some("End"),
        57425 => Some("Insert"),
        57426 => Some("Delete"),
        57441 | 57447 => Some("Shift"),
        57442 | 57448 => Some("Control"),
        57443 | 57449 => Some("Alt"),
        57444 | 57450 => Some("Meta"),
        57344.. => return None,
        _ => None,
    };
    if let Some(v) = named {
        return Some(
            KeyParts {
                mods,
                value: v.into(),
            }
            .name(),
        );
    }
    let base = char::from_u32(code).filter(|c| !c.is_control())?;
    let mut value = base.to_string();
    if mods & SHIFT != 0 {
        value = if let Some(s) = char::from_u32(shifted).filter(|_| shifted > 0) {
            s.to_string()
        } else if !text.is_empty() {
            text.to_string()
        } else {
            single(base.to_uppercase()).unwrap_or(base).to_string()
        };
        if value != base.to_string() {
            mods &= !SHIFT;
        }
    }
    Some(KeyParts { mods, value }.name())
}

/// The key a CSI sequence names.
fn csi_key(params: &str, fin: u8) -> Option<String> {
    if params.starts_with(['<', '=', '>', '?']) {
        return None;
    }
    let fields: Vec<&str> = params.split(';').collect();
    let field = |i: usize| fields.get(i).copied().unwrap_or("");
    match fin {
        b'u' => {
            let mut codes = field(0).split(':');
            let code: u32 = codes.next()?.parse().ok()?;
            let shifted: u32 = match codes.next() {
                Some(s) if !s.is_empty() => s.parse().ok()?,
                _ => 0,
            };
            let (mods, release) = mods_of(field(1))?;
            if release {
                return None;
            }
            let mut text = String::new();
            if !field(2).is_empty() {
                for c in field(2).split(':') {
                    text.push(char::from_u32(c.parse().ok()?)?);
                }
            }
            code_key(code, mods, shifted, &text)
        }
        b'~' => {
            let n: u32 = field(0).parse().ok()?;
            if n == 27 && fields.len() >= 3 {
                let (mods, release) = mods_of(fields[1])?;
                if release {
                    return None;
                }
                return code_key(fields[2].parse().ok()?, mods, 0, "");
            }
            let v = match n {
                1 | 7 => "Home",
                4 | 8 => "End",
                2 => "Insert",
                3 => "Delete",
                5 => "PageUp",
                6 => "PageDown",
                _ => return None,
            };
            let (mods, release) = mods_of(field(1))?;
            (!release).then(|| {
                KeyParts {
                    mods,
                    value: v.into(),
                }
                .name()
            })
        }
        b'Z' => Some("Shift+Tab".into()),
        f => {
            let v = final_key(f)?;
            let (mods, release) = mods_of(field(1))?;
            (!release).then(|| {
                KeyParts {
                    mods,
                    value: v.into(),
                }
                .name()
            })
        }
    }
}

/// How a host reads bindings (SPEC §10.2, §10.4): the modifier `Mod`
/// stands for, and its edit shortcuts. The default is Control with the
/// default edit shortcuts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Platform {
    /// Whether `Mod` is Meta (Command, on macOS); Control where false.
    pub meta: bool,
    /// The host's edit shortcuts, each edit action with its keys (SPEC
    /// §10.2); `None` is [`default_edit`].
    pub edit: Option<Vec<(String, Vec<String>)>>,
}

/// The edit actions, in SPEC §10.2's order.
pub const EDIT_ACTIONS: [&str; 6] = ["select-all", "copy", "cut", "paste", "undo", "redo"];

/// The default edit shortcuts for a `Mod` (SPEC §10.2), Meta where `meta`
/// and Control otherwise, the keys canonical with `Mod` read.
pub fn default_edit(meta: bool) -> Vec<(String, Vec<String>)> {
    let m = if meta { "Meta" } else { "Control" };
    EDIT_ACTIONS
        .iter()
        .zip(["a", "c", "x", "v", "z", "Z"])
        .map(|(a, k)| {
            let mut keys = vec![format!("{m}+{k}")];
            if *a == "redo" && !meta {
                keys.push("Control+y".into());
            }
            (a.to_string(), keys)
        })
        .collect()
}

impl Platform {
    /// `Mod`'s name, as capabilities report it: `"Meta"` or `"Control"`.
    pub fn mod_name(&self) -> &'static str {
        if self.meta { "Meta" } else { "Control" }
    }

    /// The edit shortcuts: the host's, or [`default_edit`].
    pub fn edit(&self) -> Vec<(String, Vec<String>)> {
        self.edit.clone().unwrap_or_else(|| default_edit(self.meta))
    }

    /// The edit action whose shortcut a pressed key is, if it is one: the
    /// key's own name, or, for a key with Shift that no shortcut names, the
    /// key without Shift, as a keymap reads it.
    pub fn edit_action(&self, key: &str) -> Option<String> {
        let mut m = Keymap {
            meta: self.meta,
            ..Keymap::default()
        };
        for (a, keys) in self.edit() {
            for k in keys {
                m.bind(&k, &a);
            }
        }
        m.bound(key).map(str::to_string)
    }
}

/// A key's name as the platform reads it: canonical, with `Mod` read.
/// `None` when it does not parse.
pub fn read_key(platform: &Platform, name: &str) -> Option<String> {
    split_key(name).map(|k| k.read_mod(platform.meta).canonical().name())
}

/// Reads a `data-key` value (SPEC §10.5): its keys separated by ASCII white
/// space, each as [`read_key`] reads it, and `<leader>` as the keys of
/// `leader`, a sequence written the same way. `None` for a value a host
/// ignores: no keys, a key that does not parse, Tab, Shift+Tab or Escape,
/// or any other name in angle brackets.
pub fn parse_sequence(platform: &Platform, leader: &str, value: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for w in value.split([' ', '\t', '\n', '\x0c', '\r']).filter(|w| !w.is_empty()) {
        if w == "<leader>" {
            out.extend(parse_sequence(platform, "", leader)?);
            continue;
        }
        if w.len() > 2 && w.starts_with('<') && w.ends_with('>') {
            return None;
        }
        let k = read_key(platform, w)?;
        if matches!(k.as_str(), "Tab" | "Shift+Tab" | "Escape") {
            return None;
        }
        out.push(k);
    }
    (!out.is_empty()).then_some(out)
}

/// Bindings of keys to actions (SPEC §10.2), read on a [`Platform`].
#[derive(Clone, Debug, Default)]
pub struct Keymap {
    /// Each key, canonical with `Mod` read, in the order first bound, and
    /// its action.
    bindings: Vec<(String, String)>,
    /// Each key's last binding as written, `Mod` kept, by index.
    written: Vec<String>,
    multiline: bool,
    /// Whether `Mod` reads as Meta.
    meta: bool,
}

/// Reads a `data-keys` value as SPEC §10.2 has hosts read it: bindings
/// separated by ASCII white space, each `key=action`, split at its last
/// `=`; dropping a key that does not parse, an action it does not know,
/// and Tab, Shift+Tab and Escape. `Mod` reads as the platform's: where it
/// is Control, `Mod+a` and `Control+a` are one key.
pub fn parse_keymap(platform: &Platform, value: &str) -> Keymap {
    let mut m = Keymap {
        meta: platform.meta,
        ..Keymap::default()
    };
    for (k, a) in bindings(value) {
        m.bind(k, a);
    }
    m
}

/// A `data-keys` value's bindings as written, in order, each split at its
/// last `=`.
fn bindings(value: &str) -> impl Iterator<Item = (&str, &str)> {
    value
        .split([' ', '\t', '\n', '\x0c', '\r'])
        .filter_map(|b| b.rfind('=').map(|i| (&b[..i], &b[i + 1..])))
}

/// The default keymap's keys on every host (SPEC §10.2); Enter, those of
/// the platform's `Mod` and the edit shortcuts follow.
const DEFAULT_KEYS: &[(&str, &str)] = &[
    ("ArrowLeft", "char-backward"),
    ("ArrowRight", "char-forward"),
    ("Alt+ArrowLeft", "word-backward"),
    ("Alt+ArrowRight", "word-forward"),
    ("Home", "line-start"),
    ("End", "line-end"),
    ("Backspace", "delete-char-backward"),
    ("Delete", "delete-char-forward"),
    ("Alt+Backspace", "delete-word-backward"),
    ("Alt+Delete", "delete-word-forward"),
    ("ArrowUp", "line-previous"),
    ("ArrowDown", "line-next"),
    ("PageUp", "page-up"),
    ("PageDown", "page-down"),
];

/// The default keymap's keys where `Mod` is Control.
const CONTROL_KEYS: &[(&str, &str)] = &[
    ("Control+ArrowLeft", "word-backward"),
    ("Control+ArrowRight", "word-forward"),
    ("Control+Home", "input-start"),
    ("Control+End", "input-end"),
    ("Control+Backspace", "delete-word-backward"),
    ("Control+Delete", "delete-word-forward"),
];

/// The default keymap's keys where `Mod` is Meta: macOS's, with Emacs's
/// Control keys.
const META_KEYS: &[(&str, &str)] = &[
    ("Meta+ArrowLeft", "line-start"),
    ("Meta+ArrowRight", "line-end"),
    ("Meta+ArrowUp", "input-start"),
    ("Meta+ArrowDown", "input-end"),
    ("Meta+Backspace", "delete-to-line-start"),
    ("Control+a", "line-start"),
    ("Control+e", "line-end"),
    ("Control+b", "char-backward"),
    ("Control+f", "char-forward"),
    ("Control+p", "line-previous"),
    ("Control+n", "line-next"),
    ("Control+h", "delete-char-backward"),
    ("Control+d", "delete-char-forward"),
    ("Control+k", "delete-to-line-end"),
];

/// A field's keymap: the default keymap for the platform's `Mod`, with its
/// edit shortcuts, for an input or a multi-line field, then each
/// `data-keys` value, the root's first. A binding to a scroll action is
/// left out where it stands, in its own value too: it neither acts nor
/// overrides an earlier binding of its key (SPEC §10.2).
pub fn resolve<'a>(
    platform: &Platform,
    multiline: bool,
    values: impl IntoIterator<Item = &'a str>,
) -> Keymap {
    let mut m = Keymap {
        multiline,
        meta: platform.meta,
        ..Keymap::default()
    };
    for (k, a) in DEFAULT_KEYS {
        m.bind(k, a);
    }
    m.bind("Enter", if multiline { "newline" } else { "submit" });
    for (k, a) in if platform.meta { META_KEYS } else { CONTROL_KEYS } {
        m.bind(k, a);
    }
    for (a, keys) in platform.edit() {
        for k in keys {
            m.bind(&k, &a);
        }
    }
    for v in values {
        for (k, a) in bindings(v).filter(|(_, a)| !scroll_action(a)) {
            m.bind(k, a);
        }
    }
    m
}

/// An element's keymap outside a text field: each `data-keys` value, the
/// root's first, with no default keymap. Only its `program` bindings and its
/// scroll actions count there ([`Keymap::program`], [`Keymap::scroll`];
/// SPEC §10.2, keys for the program, scrolling keys).
pub fn element_keymap<'a>(
    platform: &Platform,
    values: impl IntoIterator<Item = &'a str>,
) -> Keymap {
    let mut m = Keymap {
        meta: platform.meta,
        ..Keymap::default()
    };
    for v in values {
        for (k, a) in bindings(v) {
            m.bind(k, a);
        }
    }
    m
}

impl Keymap {
    /// Binds a key to an action; false, binding nothing, where a host ignores
    /// the binding.
    pub fn bind(&mut self, key: &str, action: &str) -> bool {
        let Some(w) = split_key(key).map(KeyParts::canonical) else {
            return false;
        };
        let k = w.clone().read_mod(self.meta).canonical().name();
        if !ACTIONS.contains(&action) || matches!(k.as_str(), "Tab" | "Shift+Tab" | "Escape") {
            return false;
        }
        match self.bindings.iter().position(|(b, _)| *b == k) {
            Some(i) => {
                self.bindings[i].1 = action.to_string();
                self.written[i] = w.name();
            }
            None => {
                self.bindings.push((k, action.to_string()));
                self.written.push(w.name());
            }
        }
        true
    }

    /// The keymap as a `data-keys` value: each key once, where it was first
    /// bound, as its last binding wrote it (`Mod` kept), with its last
    /// action.
    pub fn format(&self) -> String {
        self.bindings
            .iter()
            .zip(&self.written)
            .map(|((_, a), w)| format!("{w}={a}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.bindings
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, a)| a.as_str())
    }

    /// The action bound to a key, or, for a key with Shift it does not
    /// bind, to the key without Shift.
    fn bound(&self, key: &str) -> Option<&str> {
        let k = split_key(key)?.read_mod(self.meta).canonical();
        let unshifted = KeyParts {
            mods: k.mods & !SHIFT,
            value: k.value.clone(),
        };
        self.get(&k.name())
            .or_else(|| self.get(&unshifted.name()).filter(|_| k.shift()))
    }

    /// Whether the keymap gives a key to the program, before the focused
    /// element or a scroll uses it (SPEC §10.2): it binds the key to
    /// `program`, or, for a key with Shift it does not bind, the key without
    /// Shift.
    pub fn program(&self, key: &str) -> bool {
        self.bound(key) == Some("program")
    }

    /// The scroll action the keymap binds a key to, with the same fallback
    /// without Shift, or `None` (SPEC §10.2, scrolling keys). A host asks it
    /// of an element's keymap for a key the element does not use.
    pub fn scroll(&self, key: &str) -> Option<&str> {
        self.bound(key).filter(|a| scroll_action(a))
    }

    /// What a field with this keymap does with a key: an action, [`INSERT`]
    /// for a character it types, or `None` when the key is not the field's.
    pub fn lookup(&self, key: &str) -> Option<&str> {
        let k = split_key(key)?.read_mod(self.meta).canonical();
        let name = k.name();
        if matches!(name.as_str(), "Tab" | "Shift+Tab" | "Escape") {
            return None;
        }
        let unshifted = KeyParts {
            mods: k.mods & !SHIFT,
            value: k.value.clone(),
        };
        let bound = self
            .get(&name)
            .or_else(|| self.get(&unshifted.name()).filter(|_| k.shift()));
        match bound {
            Some("program") => None,
            Some(a) if multiline_action(a) && !self.multiline => None,
            Some(a) => Some(a),
            None if is_char(&k.value) && k.mods & (CONTROL | ALT | META) == 0 => Some(INSERT),
            None => None,
        }
    }

    /// Whether a field with this keymap selects with a key (SPEC §10.2,
    /// Shift selects): it looks up a move, and the key's name has Shift.
    /// The field then moves its caret and keeps its anchor.
    pub fn selects(&self, key: &str) -> bool {
        split_key(key).is_some_and(|k| k.canonical().shift())
            && self.lookup(key).is_some_and(move_action)
    }
}

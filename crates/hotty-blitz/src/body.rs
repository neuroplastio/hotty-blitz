//! The bodies of HOTTY (SPEC §3.3): the capabilities (§4), an error (§3.6)
//! and an event's detail (§9) that a host sends, and the delta (§6.1) and
//! the menu (§10.7) that a program does. Each is one msgpack map, and each
//! of its fields has a type, as the spec's tables give it: an int goes out
//! as an int, a float as a float even when it is whole (`scale: 2.0`), and
//! a field the host has nothing for is left out, never sent as nil.
//!
//! Nothing here writes what a body may not hold (§3.3), which would fail
//! the whole body for its reader: no nil (an `Option` is skipped when
//! `None`), no key but a str (fields by name, and maps from `String` or a
//! directive's name), no key twice in one map (a struct's fields have
//! names of their own, none flattened or renamed into another's, and a
//! map's keys are a `BTreeMap`'s, each directive's name its own), no str
//! but UTF-8 (Rust's), and no int further than [`MAX_INT`] from zero: an
//! int is narrow enough to stay within it, or is held there, as `area` is
//! where it is measured, a step by its count (`data-steps`, §9.1), and
//! `limits` here.

use crate::policy::Policy;
use serde::Serialize;
use std::collections::BTreeMap;

/// The largest int a body carries, either way from zero (SPEC §3.3), so
/// that a reader whose numbers are doubles holds it exactly.
pub const MAX_INT: i64 = (1 << 53) - 1;

/// `value` as msgpack: a struct as a map of its fields by name (rmp-serde
/// writes one as an array unless told), ints in their smallest form, and
/// floats in 64 bits.
pub fn encode(value: &impl Serialize) -> Vec<u8> {
    let mut out = Vec::new();
    value
        .serialize(&mut rmp_serde::Serializer::new(&mut out).with_struct_map())
        .expect("a body has no map or array of unknown length");
    out
}

/// An `f32` as the `f64` it reads as: a scale of 1.6 is sent as 1.6, not
/// as 1.600000023841858.
pub fn float(v: f32) -> f64 {
    v.to_string().parse().unwrap_or(f64::from(v))
}

/// The capabilities (SPEC §4): the body of the reply to `a=q`.
#[derive(Clone, Debug, Serialize)]
pub struct Caps {
    /// The version the host answers in, which the query listed (§4).
    pub v: &'static str,
    /// One cell, in CSS pixels.
    pub cellcss: CellCss,
    pub scale: f64,
    pub scheme: &'static str,
    /// The modifier `Mod` stands for in bindings (SPEC §10.4).
    #[serde(rename = "mod")]
    pub modifier: &'static str,
    /// The optional features the host has (§4); absent: none.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<&'static str>,
    pub limits: Limits,
    /// The host's half of the network policy (§7.2): directive to
    /// sources, empty unless its user granted something.
    pub net: Policy,
    /// The edit shortcuts (SPEC §10.2), where the terminal takes edit
    /// commands from a program in the cells (§10.6).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit: Option<Edit>,
    pub host: &'static str,
    pub version: &'static str,
}

/// The edit shortcuts (SPEC §4, §10.2): each edit action, with its keys.
pub type Edit = BTreeMap<String, Vec<String>>;

/// What a `caps` event carries (SPEC §9): the fields of the capabilities
/// that changed since the program last heard them, as the capabilities
/// have them. One that became absent comes as its type's empty value.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Changed {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cellcss: Option<CellCss>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<&'static str>,
    #[serde(rename = "mod", skip_serializing_if = "Option::is_none")]
    pub modifier: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<Vec<&'static str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub net: Option<Policy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit: Option<Edit>,
}

/// A program's menu (SPEC §10.7), myelin's `Tree` with a name for each
/// binding: what it is for, and its items in the order to show them. A
/// program sends one with `a=menu`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Menu {
    pub label: String,
    pub items: Vec<MenuItem>,
}

/// An item of a menu: the key that reaches it after its group's (none for
/// one no key reaches), what it does, the command a pick sends (none for
/// one only keys reach), whether it does anything now, and the items after
/// it, which make it a group.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuItem {
    pub key: String,
    pub label: String,
    pub name: String,
    pub enabled: bool,
    pub items: Vec<MenuItem>,
}

impl MenuItem {
    /// Whether keys follow it: it has items.
    pub fn group(&self) -> bool {
        !self.items.is_empty()
    }
}

/// How deep a menu's items nest, the top level's being one (SPEC §10.7).
pub const MENU_DEPTH: usize = 8;

/// One cell's size in CSS pixels (SPEC §4: `cellcss`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct CellCss {
    pub w: f64,
    pub h: f64,
}

/// The host's limits (SPEC §13).
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Limits {
    /// The resource store's size, in bytes: at most [`MAX_INT`] as it is
    /// sent, however large the store.
    #[serde(serialize_with = "capped")]
    pub resources: u64,
    /// The most bytes one message decodes to (SPEC §3.4, §13):
    /// [`hotty_wire::MAX_COMMAND_BYTES`].
    pub message: u64,
}

/// An int as a body carries it: at most [`MAX_INT`].
fn capped<S: serde::Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_u64((*v).min(MAX_INT as u64))
}

/// The body of `a=err` (SPEC §3.6).
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Error<'a> {
    pub code: &'a str,
    pub detail: &'a str,
}

/// The cells an element covers (SPEC §9: `area`), counted from the
/// surface's top left cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Area {
    pub c: i64,
    pub r: i64,
    pub w: i64,
    pub h: i64,
}

/// An event's detail (SPEC §9), as its kind has it. An event with nothing
/// to say has none: [`crate::Event::detail`] is `None`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Detail {
    /// `click`: where the element is, the modifier keys held, a link's
    /// `href` and `url`, and the element's `value` attribute.
    Click {
        #[serde(skip_serializing_if = "Option::is_none")]
        area: Option<Area>,
        /// The modifier bits (§9), left out when none is held.
        #[serde(skip_serializing_if = "Option::is_none")]
        mods: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        href: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
        /// Its place in its run of clicks, from 2 (SPEC §9).
        #[serde(skip_serializing_if = "Option::is_none")]
        count: Option<u32>,
    },
    /// `press`: where the element `t` names is, and the modifier keys
    /// held; each is left out when the press has none.
    Press {
        #[serde(skip_serializing_if = "Option::is_none")]
        area: Option<Area>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mods: Option<u8>,
    },
    /// `change` of a checkbox or a radio button: whether it is checked,
    /// and its `value` attribute.
    Checked {
        checked: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    },
    /// `change` of any other control, and `input`.
    Value { value: String },
    /// `submit`: the form's fields, each its name and its value, in tree
    /// order.
    Fields { fields: Vec<(String, String)> },
    /// `dragstart`, `drag` and `dragend` (§9.1): the pointer's cell and
    /// the modifier keys held, its step in an element with `data-steps`,
    /// and its step in a drop target with them (`tx`, `ty`).
    Drag {
        c: i32,
        r: i32,
        #[serde(skip_serializing_if = "Option::is_none")]
        mods: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        x: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        y: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tx: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        ty: Option<u64>,
    },
    /// `fit`: the rows `r=auto` would choose now.
    Fit { r: u16 },
    /// `hover` over the window (§9.4): the pointer's cell.
    Hover { c: i32, r: i32 },
    /// `hover` when the pointer left the window: `out` is true.
    Out { out: bool },
    /// `view` (§9.5): the elements in view, and whether the root shows
    /// its end.
    View(View),
    /// `command` (§10.6): the edit command, its key, and a paste's text.
    Command {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// `caps` (§9): the capabilities that changed.
    Caps(Changed),
}

/// What a `view` carries (SPEC §9.5): each element in view by its id, with
/// its cells, the whole of it however little is shown; and `end`, whether
/// the document's root is scrolled as far down as it goes. Its own name,
/// `in`, is a keyword here.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct View {
    #[serde(rename = "in")]
    pub inside: BTreeMap<String, Area>,
    pub end: bool,
}

/// A value of a body a program sent, read by [`read`].
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Array(Vec<Value>),
    /// A map's fields in the order they came, each name once.
    Map(Vec<(String, Value)>),
    /// A form of msgpack that no field of HOTTY has (bin, an extension): a
    /// field a reader does not know may hold one.
    Other,
}

impl Value {
    fn field(&self, name: &str) -> Option<&Value> {
        match self {
            Value::Map(m) => m.iter().find(|(k, _)| k == name).map(|(_, v)| v),
            _ => None,
        }
    }

    fn str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
}

/// How deep a body may nest, its map being the first level (SDK.md §3.9):
/// so that no body exhausts a reader's stack.
const LEVELS: usize = 32;

/// The most values a body may hold, however many bytes it takes, so that
/// a body of one-byte values cannot make the host allocate a value for
/// each of its 64 MiB.
const VALUES: usize = 1 << 22;

/// Reads a body a program sent (SPEC §3.3): one msgpack map, with nothing
/// after it, nested at most [`LEVELS`] deep. A body that breaks a rule of
/// §3.3 anywhere in it, in a field a reader knows or not, does not decode:
/// a nil, a key that is not a str or is given twice in one map (keys
/// compared as the strs they are, whatever their form), a str that is not
/// UTF-8, an int further than [`MAX_INT`] from zero, or a timestamp of
/// another size, with a second's nanoseconds or more, or with seconds
/// further than [`MAX_INT`] from 1970. Nothing is allocated for a length a
/// body claims: only for the values it holds.
pub fn read(body: &[u8]) -> Result<Value, String> {
    let mut r = Reader { b: body, at: 0, values: 0 };
    let v = r.value(1)?;
    if r.at < body.len() {
        return Err(format!("{} byte(s) after the body", body.len() - r.at));
    }
    if !matches!(v, Value::Map(_)) {
        return Err("the body is not a map".into());
    }
    Ok(v)
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
    values: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.b.len());
        let end = end.ok_or_else(|| format!("is cut short at byte {}", self.at))?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }

    /// A big-endian unsigned int of `n` bytes.
    fn uint(&mut self, n: usize) -> Result<u64, String> {
        Ok(self.take(n)?.iter().fold(0, |v, &x| v << 8 | u64::from(x)))
    }

    fn len(&mut self, n: usize) -> Result<usize, String> {
        self.uint(n).map(|v| usize::try_from(v).unwrap_or(usize::MAX))
    }

    /// One value at `level` (the body's map is 1).
    fn value(&mut self, level: usize) -> Result<Value, String> {
        self.values += 1;
        if self.values > VALUES {
            return Err(format!("holds more than {VALUES} values"));
        }
        let at = self.at;
        let tag = self.take(1)?[0];
        match tag {
            0x00..=0x7f => Ok(Value::Int(tag.into())),
            0xe0..=0xff => Ok(Value::Int(i64::from(tag as i8))),
            0xcc..=0xcf => {
                let v = self.uint(1 << (tag - 0xcc))?;
                int(i128::from(v), at)
            }
            0xd0..=0xd3 => {
                let n = 1 << (tag - 0xd0);
                let shift = 64 - 8 * n as u32;
                let v = ((self.uint(n)? << shift) as i64) >> shift;
                int(i128::from(v), at)
            }
            0xca => Ok(Value::Float(f64::from(f32::from_bits(self.uint(4)? as u32)))),
            0xcb => Ok(Value::Float(f64::from_bits(self.uint(8)?))),
            0xc0 => Err(format!("holds a nil at byte {at}")),
            0xc2 => Ok(Value::Bool(false)),
            0xc3 => Ok(Value::Bool(true)),
            0xa0..=0xbf => self.str((tag & 0x1f).into(), at).map(Value::Str),
            0xd9 => self.len(1).and_then(|n| self.str(n, at)).map(Value::Str),
            0xda => self.len(2).and_then(|n| self.str(n, at)).map(Value::Str),
            0xdb => self.len(4).and_then(|n| self.str(n, at)).map(Value::Str),
            0xc4 => self.len(1).and_then(|n| self.take(n)).map(|_| Value::Other),
            0xc5 => self.len(2).and_then(|n| self.take(n)).map(|_| Value::Other),
            0xc6 => self.len(4).and_then(|n| self.take(n)).map(|_| Value::Other),
            0x90..=0x9f => self.array((tag & 0x0f).into(), level, at),
            0xdc => self.len(2).and_then(|n| self.array(n, level, at)),
            0xdd => self.len(4).and_then(|n| self.array(n, level, at)),
            0x80..=0x8f => self.map((tag & 0x0f).into(), level, at),
            0xde => self.len(2).and_then(|n| self.map(n, level, at)),
            0xdf => self.len(4).and_then(|n| self.map(n, level, at)),
            0xd4..=0xd8 => self.ext(1 << (tag - 0xd4), at),
            0xc7 => self.len(1).and_then(|n| self.ext(n, at)),
            0xc8 => self.len(2).and_then(|n| self.ext(n, at)),
            0xc9 => self.len(4).and_then(|n| self.ext(n, at)),
            0xc1 => Err(format!("is not msgpack: 0xc1 at byte {at}")),
        }
    }

    fn str(&mut self, n: usize, at: usize) -> Result<String, String> {
        let s = self.take(n)?;
        std::str::from_utf8(s)
            .map(str::to_string)
            .map_err(|_| format!("holds a str that is not UTF-8 at byte {at}"))
    }

    /// A map or an array at `level`: no deeper than [`LEVELS`].
    fn nest(&self, level: usize, at: usize) -> Result<usize, String> {
        if level > LEVELS {
            return Err(format!("is nested more than {LEVELS} levels deep at byte {at}"));
        }
        Ok(level + 1)
    }

    fn array(&mut self, n: usize, level: usize, at: usize) -> Result<Value, String> {
        let inner = self.nest(level, at)?;
        let mut a = Vec::new();
        for _ in 0..n {
            a.push(self.value(inner)?);
        }
        Ok(Value::Array(a))
    }

    fn map(&mut self, n: usize, level: usize, at: usize) -> Result<Value, String> {
        let inner = self.nest(level, at)?;
        let mut m: Vec<(String, Value)> = Vec::new();
        let mut names = std::collections::HashSet::new();
        for _ in 0..n {
            let key = self.at;
            if !matches!(self.b.get(key), Some(0xa0..=0xbf | 0xd9..=0xdb)) {
                return Err(format!("holds a key that is not a str at byte {key}"));
            }
            let Value::Str(k) = self.value(inner)? else {
                unreachable!("a str reads as a str");
            };
            if !names.insert(k.clone()) {
                return Err(format!("holds the key {k:?} twice in one map, again at byte {key}"));
            }
            m.push((k, self.value(inner)?));
        }
        Ok(Value::Map(m))
    }

    /// An extension of `n` bytes after its type: a timestamp is checked,
    /// any other is skipped.
    fn ext(&mut self, n: usize, at: usize) -> Result<Value, String> {
        let ty = self.take(1)?[0] as i8;
        let data = self.take(n)?;
        if ty != -1 {
            return Ok(Value::Other);
        }
        let be = |b: &[u8]| b.iter().fold(0u64, |v, &x| v << 8 | u64::from(x));
        let (sec, nsec) = match data.len() {
            4 => (be(data) as i64, 0),
            8 => ((be(data) & 0x3_ffff_ffff) as i64, be(data) >> 34),
            12 => (be(&data[4..]) as i64, be(&data[..4])),
            len => return Err(format!("holds a timestamp of {len} bytes at byte {at}")),
        };
        if nsec >= 1_000_000_000 {
            return Err(format!("holds a timestamp of a second's nanoseconds or more at byte {at}"));
        }
        if sec.unsigned_abs() > MAX_INT as u64 {
            return Err(format!("holds a timestamp past 2^53 − 1 seconds from 1970 at byte {at}"));
        }
        Ok(Value::Other)
    }
}

/// An int, if a body may carry it.
fn int(v: i128, at: usize) -> Result<Value, String> {
    if v.unsigned_abs() > MAX_INT as u128 {
        return Err(format!("holds an int further than 2^53 − 1 from zero at byte {at}"));
    }
    Ok(Value::Int(v as i64))
}

/// What a delta op does (SPEC §6.1), by the table's names.
pub const OPS: &[&str] = &[
    "morph", "inner", "replace", "append", "prepend", "before", "after", "remove", "attr",
    "unattr", "text", "var", "reveal",
];

/// One op of a delta (SPEC §6.1): what it does, its target, its name `k`
/// and its value `v`. Each is absent when the op leaves it out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Op {
    pub op: String,
    pub t: Option<String>,
    pub k: Option<String>,
    pub v: Option<String>,
}

/// Why a delta is refused whole (SPEC §6.1): `EINVAL`, with the index of
/// the first op at fault, where one is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fault {
    pub index: Option<usize>,
    pub detail: String,
}

/// Reads and checks a delta's body (SPEC §6.1) whole, before any op
/// applies: it decodes ([`read`]), is `{"ops": [op]}` with at least one op,
/// each op a map that names an op of the table, holds `op`, `t`, `k` and
/// `v` as strs when it has them (whether or not the op takes them), has
/// the `t` the op needs (every one but `morph`, whose empty `t` is none)
/// and the `k` of `attr`, `unattr` and `var`, neither empty, and, for
/// `reveal`, a `v` of `start`, `end` or none. A `v` an op takes and lacks
/// is empty, one it does not take is ignored, as is a key the table does
/// not name.
pub fn decode_delta(body: &[u8]) -> Result<Vec<Op>, Fault> {
    let whole = |detail: String| Fault { index: None, detail };
    let root = read(body).map_err(|e| whole(format!("the delta's body does not decode: {e}")))?;
    let ops = match root.field("ops") {
        Some(Value::Array(a)) if !a.is_empty() => a,
        Some(Value::Array(_)) => return Err(whole("the delta has no ops".into())),
        _ => return Err(whole("the delta's body is not {\"ops\": [op]}".into())),
    };
    let mut out = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        let at = |detail: String| Fault { index: Some(i), detail };
        if !matches!(op, Value::Map(_)) {
            return Err(at(format!("op {i} is not a map")));
        }
        // A field an op takes is a str when it is there.
        let text = |name: &str| match op.field(name) {
            None => Ok(None),
            Some(Value::Str(s)) => Ok(Some(s.clone())),
            Some(_) => Err(at(format!("op {i}: {name} is not a str"))),
        };
        let name = text("op")?.ok_or_else(|| at(format!("op {i} names no op")))?;
        if !OPS.contains(&name.as_str()) {
            return Err(at(format!("op {i}: unknown op {name}")));
        }
        // Every key read is a str when it is there, whether the op takes it.
        let (t, k, v) = (text("t")?, text("k")?, text("v")?);
        if name != "morph" && t.as_deref().is_none_or(str::is_empty) {
            return Err(at(format!("op {i}: {name} needs t")));
        }
        if k.as_deref().is_none_or(str::is_empty)
            && matches!(name.as_str(), "attr" | "unattr" | "var")
        {
            return Err(at(format!("op {i}: {name} needs k")));
        }
        // An empty `t` of a morph is no target: the whole tree.
        let t = t.filter(|t| !t.is_empty());
        let takes_v = !matches!(name.as_str(), "remove" | "unattr");
        let v = v.filter(|_| takes_v);
        if name == "reveal" && v.as_deref().is_some_and(|v| !matches!(v, "start" | "end")) {
            return Err(at(format!("op {i}: reveal takes start, end or nothing")));
        }
        out.push(Op { op: name, t, k, v });
    }
    Ok(out)
}

/// A str field of a body's map: empty when it is not there.
fn text(m: &Value, name: &str) -> Result<String, String> {
    match m.field(name) {
        None => Ok(String::new()),
        Some(Value::Str(s)) => Ok(s.clone()),
        Some(_) => Err(format!("{name} is not a str")),
    }
}

/// An array field of a body's map: empty when it is not there.
fn list<'a>(m: &'a Value, name: &str) -> Result<&'a [Value], String> {
    match m.field(name) {
        None => Ok(&[]),
        Some(Value::Array(a)) => Ok(a.as_slice()),
        Some(_) => Err(format!("{name} is not an array")),
    }
}

/// Reads a menu's body (SPEC §10.7): one msgpack map ([`read`]),
/// `{"label": str, "items": [item]}` with each item `{"key": str,
/// "label": str, "name": str, "enabled": bool, "items": [item]}`. A field
/// left out is empty, and false for `enabled`; one that holds another type
/// fails the menu, and so do items nested past [`MENU_DEPTH`].
pub fn decode_menu(body: &[u8]) -> Result<Menu, String> {
    let root = read(body)?;
    fn items(of: &Value, depth: usize) -> Result<Vec<MenuItem>, String> {
        let list = list(of, "items")?;
        if depth > MENU_DEPTH && !list.is_empty() {
            return Err(format!("items nest more than {MENU_DEPTH} deep"));
        }
        list.iter()
            .map(|i| {
                if !matches!(i, Value::Map(_)) {
                    return Err("an item is not a map".to_string());
                }
                Ok(MenuItem {
                    key: text(i, "key")?,
                    label: text(i, "label")?,
                    name: text(i, "name")?,
                    enabled: match i.field("enabled") {
                        None => false,
                        Some(Value::Bool(b)) => *b,
                        Some(_) => return Err("enabled is not a bool".to_string()),
                    },
                    items: items(i, depth + 1)?,
                })
            })
            .collect()
    }
    Ok(Menu {
        label: text(&root, "label")?,
        items: items(&root, 1)?,
    })
}

/// Reads the body of `a=menu:show=1` (SPEC §10.7): `{"seq": [str]}`, the
/// keys typed so far. No body is none.
pub fn decode_seq(body: &[u8]) -> Result<Vec<String>, String> {
    if body.is_empty() {
        return Ok(Vec::new());
    }
    list(&read(body)?, "seq")?
        .iter()
        .map(|k| k.str().map(str::to_string).ok_or_else(|| "seq holds a non-str".to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_is_a_map_by_name_and_a_whole_float_stays_a_float() {
        // {"w": 720.0, "h": 2.5}: float 64 for both.
        assert_eq!(
            encode(&CellCss { w: 720.0, h: 2.5 }),
            b"\x82\xa1w\xcb\x40\x86\x80\0\0\0\0\0\xa1h\xcb\x40\x04\0\0\0\0\0\0"
        );
        // {"c": -1, "r": 300}: ints in their smallest form, no mods, x or y.
        assert_eq!(
            encode(&Detail::Drag {
                c: -1,
                r: 300,
                mods: None,
                x: None,
                y: None,
                tx: None,
                ty: None
            }),
            b"\x82\xa1c\xff\xa1r\xcd\x01\x2c"
        );
        // {"fields": [["a", "1"], ["a", "2"]]}: pairs, a name twice.
        assert_eq!(
            encode(&Detail::Fields {
                fields: vec![("a".into(), "1".into()), ("a".into(), "2".into())]
            }),
            b"\x81\xa6fields\x92\x92\xa1a\xa11\x92\xa1a\xa12"
        );
        assert_eq!(float(1.6), 1.6);
        // {"in": {"a": {"c": 0, "r": -2, "w": 4, "h": 1}}, "end": false}:
        // `in` by its name, and a row above the surface a negative int.
        let view = View {
            inside: [(
                "a".to_string(),
                Area {
                    c: 0,
                    r: -2,
                    w: 4,
                    h: 1,
                },
            )]
            .into(),
            end: false,
        };
        assert_eq!(
            encode(&Detail::View(view)),
            b"\x82\xa2in\x81\xa1a\x84\xa1c\x00\xa1r\xfe\xa1w\x04\xa1h\x01\xa3end\xc2"
        );
    }

    #[test]
    fn a_limit_past_the_ints_a_body_carries_is_the_most_it_carries() {
        // {"resources": 2^53 - 1, "message": 1 MiB}, resources as uint 64.
        let most = b"\x82\xa9resources\xcf\x00\x1f\xff\xff\xff\xff\xff\xff\xa7message\xce\x00\x10\0\0";
        for resources in [MAX_INT as u64, MAX_INT as u64 + 1, u64::MAX] {
            assert_eq!(encode(&Limits { resources, message: 1 << 20 }), most, "{resources}");
        }
        assert_eq!(
            encode(&Limits {
                resources: 64 << 20,
                message: 1 << 20
            }),
            b"\x82\xa9resources\xce\x04\0\0\0\xa7message\xce\x00\x10\0\0"
        );
    }
}

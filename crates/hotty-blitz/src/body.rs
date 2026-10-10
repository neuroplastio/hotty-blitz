//! The bodies a host sends (SPEC §3.3): the capabilities (§4), an error
//! (§3.6) and an event's detail (§9). Each is one msgpack map, and each of
//! its fields has a type, as the spec's tables give it: an int goes out as
//! an int, a float as a float even when it is whole (`scale: 2.0`), and a
//! field the host has nothing for is left out, never sent as nil.
//!
//! Nothing here writes what a body may not hold (§3.3), which would fail
//! the whole body for its reader: no nil (an `Option` is skipped when
//! `None`), no key but a str (fields by name, and maps from `String` or a
//! directive's name), no str but UTF-8 (Rust's), and no int further than
//! [`MAX_INT`] from zero: an int is narrow enough to stay within it, or is
//! held there, as `area` is where it is measured, a step by its count
//! (`data-steps`, §9.1), and `limits` here.

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
    pub v: &'static str,
    pub ops: &'static [&'static str],
    pub events: &'static [&'static str],
    pub cell: Cell,
    pub scale: f64,
    pub scheme: &'static str,
    pub limits: Limits,
    /// The host's half of the network policy (§7.2): directive to
    /// sources, empty unless its user granted something.
    pub net: Policy,
    #[serde(skip_serializing_if = "is_false")]
    pub passthrough: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub scroll: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub steps: bool,
    pub host: &'static str,
    pub version: &'static str,
}

/// The cell size in device pixels.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Cell {
    pub w: u32,
    pub h: u32,
}

/// The host's limits (SPEC §13).
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Limits {
    /// The resource store's size, in bytes: at most [`MAX_INT`] as it is
    /// sent, however large the store.
    #[serde(serialize_with = "capped")]
    pub resources: u64,
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
    /// `click`: where the element is, a link's `href` and `url`, and the
    /// element's `value` attribute.
    Click {
        #[serde(skip_serializing_if = "Option::is_none")]
        area: Option<Area>,
        #[serde(skip_serializing_if = "Option::is_none")]
        href: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    },
    /// `press`: where the element `t` names is.
    Press { area: Area },
    /// `change` of a checkbox or a radio button: whether it is checked,
    /// and its `value` attribute.
    Checked {
        checked: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    },
    /// `change` of any other control, and `input`.
    Value { value: String },
    /// `submit`: the form's fields, names to values.
    Fields(BTreeMap<String, String>),
    /// `dragstart`, `drag` and `dragend` (§9.1): the pointer's cell and
    /// the modifier keys held, and its step in an element with
    /// `data-steps`.
    Drag {
        c: i32,
        r: i32,
        keys: Vec<&'static str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        x: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        y: Option<u64>,
    },
    /// `resize`: the surface's size in CSS pixels.
    Resize { w: f64, h: f64 },
    /// `fit`: the rows `r=auto` would choose now.
    Fit { r: u16 },
    /// `hover` over the window (§9.4): the pointer's cell.
    Hover { c: i32, r: i32 },
    /// `hover` when the pointer left the window: `out` is true.
    Out { out: bool },
}

fn is_false(b: &bool) -> bool {
    !b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_is_a_map_by_name_and_a_whole_float_stays_a_float() {
        // {"w": 720.0, "h": 2.5}: float 64 for both.
        assert_eq!(
            encode(&Detail::Resize { w: 720.0, h: 2.5 }),
            b"\x82\xa1w\xcb\x40\x86\x80\0\0\0\0\0\xa1h\xcb\x40\x04\0\0\0\0\0\0"
        );
        // {"c": -1, "r": 300, "keys": []}: ints in their smallest form, and
        // no x or y.
        assert_eq!(
            encode(&Detail::Drag {
                c: -1,
                r: 300,
                keys: vec![],
                x: None,
                y: None
            }),
            b"\x83\xa1c\xff\xa1r\xcd\x01\x2c\xa4keys\x90"
        );
        assert_eq!(float(1.6), 1.6);
    }

    #[test]
    fn a_limit_past_the_ints_a_body_carries_is_the_most_it_carries() {
        // {"resources": 2^53 - 1}, as uint 64.
        let most = b"\x81\xa9resources\xcf\x00\x1f\xff\xff\xff\xff\xff\xff";
        for resources in [MAX_INT as u64, MAX_INT as u64 + 1, u64::MAX] {
            assert_eq!(encode(&Limits { resources }), most, "{resources}");
        }
        assert_eq!(
            encode(&Limits {
                resources: 64 << 20
            }),
            b"\x81\xa9resources\xce\x04\0\0\0"
        );
    }
}

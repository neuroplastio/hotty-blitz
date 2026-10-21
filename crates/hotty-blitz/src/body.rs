//! The details of the events a host sends (SPEC §9), as hotty-blitz has
//! them: one type for each kind's. The hotty crate's host side writes them
//! (`hotty::host::encode_event`), and the capabilities and errors too
//! (`caps_body`, `reply_err`), each body one msgpack map with each field of
//! the type the spec's tables give it, nothing in it that a body may not
//! hold (SPEC §3.3); it reads what a program sends (`delta_of`, `menu_of`,
//! `touch_of`).

use std::collections::BTreeMap;

/// The largest int a body carries, either way from zero (SPEC §3.3), so
/// that a reader whose numbers are doubles holds it exactly.
pub const MAX_INT: i64 = (1 << 53) - 1;

/// An `f32` as the `f64` it reads as: a scale of 1.6 is sent as 1.6, not
/// as 1.600000023841858.
pub fn float(v: f32) -> f64 {
    v.to_string().parse().unwrap_or(f64::from(v))
}

/// The edit shortcuts (SPEC §4, §10.2): each edit action, with its keys.
pub type Edit = BTreeMap<String, Vec<String>>;

/// The cells an element covers (SPEC §9: `area`), counted from the
/// surface's top left cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub c: i64,
    pub r: i64,
    pub w: i64,
    pub h: i64,
}

/// An event's detail (SPEC §9), as its kind has it. An event with nothing
/// to say has none: [`crate::Event::detail`] is `None`.
#[derive(Clone, Debug, PartialEq)]
pub enum Detail {
    /// `click`: where the element is, the modifier keys held, a link's
    /// `href` and `url`, and the element's `value` attribute.
    Click {
        area: Option<Area>,
        /// The modifier bits (§9), left out when none is held.
        mods: Option<u8>,
        href: Option<String>,
        url: Option<String>,
        value: Option<String>,
        /// Its place in its run of clicks, from 2 (SPEC §9).
        count: Option<u32>,
    },
    /// `press`: where the element `t` names is, and the modifier keys
    /// held; each is left out when the press has none.
    Press {
        area: Option<Area>,
        mods: Option<u8>,
    },
    /// `change` of a checkbox or a radio button: whether it is checked,
    /// and its `value` attribute.
    Checked {
        checked: bool,
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
        mods: Option<u8>,
        x: Option<u64>,
        y: Option<u64>,
        tx: Option<u64>,
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
        key: Option<String>,
        text: Option<String>,
    },
    /// `caps` (§9): the capabilities that changed, as the capabilities
    /// have them; one that became absent as its type's empty value.
    Caps(hotty::Caps),
}

/// What a `view` carries (SPEC §9.5): each element in view by its id, with
/// its cells, the whole of it however little is shown; and `end`, whether
/// the document's root is scrolled as far down as it goes. Its own name,
/// `in`, is a keyword here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub inside: BTreeMap<String, Area>,
    pub end: bool,
}


fn area(a: &Area) -> hotty::Area {
    hotty::Area {
        c: a.c,
        r: a.r,
        w: a.w,
        h: a.h,
    }
}

/// The event a program hears, for the hotty crate to write
/// (`hotty::host::encode_event`): its kind, the surface it is of (none for
/// `command` and `caps`), the element it names (empty for none), and what
/// its detail carries.
pub fn event(kind: &str, surface: Option<&str>, target: &str, detail: Option<&Detail>) -> hotty::Event {
    let mut e = hotty::Event {
        kind: kind.to_string(),
        surface: surface.map(str::to_string),
        target: target.to_string(),
        ..hotty::Event::default()
    };
    let Some(d) = detail else {
        return e;
    };
    let mods = |m: &Option<u8>| m.map_or(0, u32::from);
    match d {
        Detail::Click {
            area: a,
            mods: m,
            href,
            url,
            value,
            count,
        } => {
            e.area = a.as_ref().map(area);
            e.mods = mods(m);
            e.link = href.clone().map(|href| hotty::Link {
                href,
                url: url.clone(),
            });
            e.value = value.clone();
            e.count = *count;
        }
        Detail::Press { area: a, mods: m } => {
            e.area = a.as_ref().map(area);
            e.mods = mods(m);
        }
        Detail::Checked { checked, value } => {
            e.checked = Some(*checked);
            e.value = value.clone();
        }
        Detail::Value { value } => e.value = Some(value.clone()),
        Detail::Fields { fields } => e.fields = Some(fields.clone()),
        Detail::Drag {
            c,
            r,
            mods: m,
            x,
            y,
            tx,
            ty,
        } => {
            let step = |v: &Option<u64>| v.map(|v| v.min(MAX_INT as u64) as i64);
            e.mods = mods(m);
            e.drag = Some(hotty::Drag {
                c: i64::from(*c),
                r: i64::from(*r),
                x: step(x),
                y: step(y),
                tx: step(tx),
                ty: step(ty),
            });
        }
        Detail::Fit { r } => e.fit_rows = Some(u32::from(*r)),
        Detail::Hover { c, r } => {
            e.hover = Some(hotty::Hover {
                c: Some(i64::from(*c)),
                r: Some(i64::from(*r)),
                out: false,
            })
        }
        Detail::Out { .. } => {
            e.hover = Some(hotty::Hover {
                c: None,
                r: None,
                out: true,
            })
        }
        Detail::View(v) => {
            e.view = Some(hotty::View {
                areas: v.inside.iter().map(|(id, a)| (id.clone(), area(a))).collect(),
                at_end: v.end,
            })
        }
        Detail::Command { name, key, text } => {
            e.command = Some(hotty::Command {
                name: name.clone(),
                key: key.clone().unwrap_or_default(),
                text: text.clone().unwrap_or_default(),
                has_text: text.is_some(),
            })
        }
        Detail::Caps(c) => e.changed = Some(c.clone()),
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    // The detail of an event as the hotty crate writes it.
    fn detail(kind: &str, d: &Detail) -> Vec<u8> {
        let out = hotty::host::encode_event(&event(kind, Some("s"), "", Some(d)));
        match hotty::Decoder::new().feed(out.as_bytes()) {
            hotty::Fed::Complete(m) => m.payload,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_body_is_a_map_by_name_and_a_whole_float_stays_a_float() {
        // {"cellcss": {"w": 720.0, "h": 2.5}}: float 64 for both.
        let caps = hotty::Caps {
            cell_css: Some(hotty::CellCss { w: 720.0, h: 2.5 }),
            ..hotty::Caps::default()
        };
        assert_eq!(
            hotty::host::caps_body(&caps),
            b"\x81\xa7cellcss\x82\xa1w\xcb\x40\x86\x80\0\0\0\0\0\xa1h\xcb\x40\x04\0\0\0\0\0\0"
        );
        // {"c": -1, "r": 300}: ints in their smallest form, no mods, x or y.
        let drag = Detail::Drag {
            c: -1,
            r: 300,
            mods: None,
            x: None,
            y: None,
            tx: None,
            ty: None,
        };
        assert_eq!(detail("drag", &drag), b"\x82\xa1c\xff\xa1r\xcd\x01\x2c");
        // {"fields": [["a", "1"], ["a", "2"]]}: pairs, a name twice.
        let fields = Detail::Fields {
            fields: vec![("a".into(), "1".into()), ("a".into(), "2".into())],
        };
        assert_eq!(
            detail("submit", &fields),
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
            detail("view", &Detail::View(view)),
            b"\x82\xa2in\x81\xa1a\x84\xa1c\x00\xa1r\xfe\xa1w\x04\xa1h\x01\xa3end\xc2"
        );
    }

    #[test]
    fn a_limit_past_the_ints_a_body_carries_is_the_most_it_carries() {
        // {"limits": {"resources": 2^53 - 1}}, as uint 64.
        let most = b"\x81\xa6limits\x81\xa9resources\xcf\x00\x1f\xff\xff\xff\xff\xff\xff";
        for resources in [MAX_INT, MAX_INT + 1, i64::MAX] {
            let caps = hotty::Caps {
                limits: Some([("resources".to_string(), resources)].into()),
                ..hotty::Caps::default()
            };
            assert_eq!(hotty::host::caps_body(&caps), most, "{resources}");
        }
    }
}

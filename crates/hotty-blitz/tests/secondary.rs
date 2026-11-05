//! A secondary press (SPEC §9, §9.1, §9.2): `Host::secondary` for the
//! mouse's secondary button and a pen's barrel button, and a touch's long
//! press. The shared vectors cover the common path; these the corners.

mod common;

use common::body;
use hotty::Message;
use hotty::host::{Output, Splitter};
use hotty_blitz::{Config, Effect, Host, Metrics, Mods, PointerKind, Touch, TouchPhase};
use serde_json::{Value, json};

fn host() -> Host {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    h.set_backdrops(true);
    h
}

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// Boxes in cells of 10×20 px, on a surface of 30×4 cells.
const DOC: &str = "<style>body{margin:0}\
    b,i,u,input{position:absolute;display:block;box-sizing:border-box;margin:0;padding:0;\
    font-style:normal;font-weight:normal;text-decoration:none;height:20px}\
    #o{left:0;top:0;width:90px;height:60px}\
    #i{left:30px;top:20px;width:30px}\
    #n{left:100px;top:0;width:30px}\
    #k{left:0;top:60px;width:30px}\
    #f{left:100px;top:40px;width:60px}\
    #a{left:100px;top:20px;width:30px}\
    #m{left:200px;top:0;width:30px}\
    #z{left:200px;top:20px;width:30px}</style>\
    <b id=o data-on=secondary><u id=i data-on=secondary>i</u></b>\
    <i id=n>n</i><i id=k data-on=\"secondary click\">k</i><input id=f type=text>\
    <b id=a data-on=\"alt secondary\">a</b>\
    <i id=m data-on=secondary><span>s</span></i>\
    <u data-on=secondary style=\"left:200px;top:40px;width:30px\">x</u>\
    <u id=z data-on=click>z</u>";

/// A placed, drawn surface `x`, with presses asked for as `p` says.
fn show(h: &mut Host, html: &str, p: &str, extra: &[(&str, &str)]) {
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], html));
    let mut pairs = vec![
        ("a", "place"),
        ("s", "x"),
        ("c", "30"),
        ("r", "4"),
        ("p", p),
        ("v", "1"),
        ("q", "2"),
    ];
    pairs.extend_from_slice(extra);
    h.handle(&cmd(&pairs, ""));
    h.render_dirty(&mut |_, _, _| {});
}

/// The events among effects: `e`, `t` (empty for none) and the body.
fn events(effects: &[Effect]) -> Vec<(String, String, Value)> {
    let mut out = Vec::new();
    let mut s = Splitter::with_max(hotty_blitz::MAX_COMMAND_BYTES);
    for e in effects {
        if let Effect::Reply(b) = e {
            s.feed(b, &mut |ev| {
                if let Output::Command(c) = ev
                    && c.get("a") == Some("ev")
                {
                    out.push((
                        c.get("e").unwrap_or("").to_string(),
                        c.get("t").unwrap_or("").to_string(),
                        body(&c.payload),
                    ));
                }
            });
        }
    }
    out
}

fn kinds(effects: &[Effect]) -> Vec<String> {
    events(effects)
        .into_iter()
        .map(|(e, t, _)| format!("{e} {t}"))
        .collect()
}

fn none() -> Mods {
    Mods::default()
}

fn with(f: impl FnOnce(&mut Mods)) -> Mods {
    let mut m = Mods::default();
    f(&mut m);
    m
}

/// A secondary press at the centre of cell (`c`, `r`).
fn press(h: &mut Host, c: u32, r: u32, mods: Mods) -> (bool, Vec<Effect>) {
    let out = h.secondary("x", c as f32 * 10.0 + 5.0, r as f32 * 20.0 + 10.0, mods);
    (out.taken, out.effects)
}

fn primary(h: &mut Host, kind: PointerKind, c: u32, r: u32) -> Vec<Effect> {
    h.pointer(
        "x",
        kind,
        c as f32 * 10.0 + 5.0,
        r as f32 * 20.0 + 10.0,
        none(),
    )
}

#[test]
fn the_innermost_opt_in_wins_and_its_press_has_button_area_and_cell() {
    for p in ["0", "1"] {
        let mut h = host();
        show(&mut h, DOC, p, &[]);
        let (taken, fx) = press(&mut h, 4, 1, none());
        assert!(taken);
        assert_eq!(
            events(&fx),
            [(
                "press".into(),
                "i".into(),
                json!({"button": 2, "area": {"c": 3, "r": 1, "w": 3, "h": 1}, "c": 4, "r": 1})
            )]
        );
        // The outer one where the inner is not.
        let (taken, fx) = press(&mut h, 1, 2, none());
        assert!(taken);
        assert_eq!(
            events(&fx),
            [(
                "press".into(),
                "o".into(),
                json!({"button": 2, "area": {"c": 0, "r": 0, "w": 9, "h": 3}, "c": 1, "r": 2})
            )]
        );
    }
}

#[test]
fn mods_are_present_when_held_and_absent_when_not() {
    let mut h = host();
    show(&mut h, DOC, "0", &[]);
    let (_, fx) = press(&mut h, 4, 1, with(|m| m.ctrl = true));
    assert_eq!(events(&fx)[0].2["mods"], 4);
    let (_, fx) = press(&mut h, 4, 1, with(|m| m.super_ = true));
    assert_eq!(events(&fx)[0].2["mods"], 8);
    let (_, fx) = press(&mut h, 4, 1, none());
    assert!(events(&fx)[0].2.get("mods").is_none());
}

#[test]
fn an_opted_in_element_without_an_id_has_no_t() {
    let mut h = host();
    show(&mut h, DOC, "1", &[]);
    let (taken, fx) = press(&mut h, 20, 2, none());
    assert!(taken);
    // No `t`, so no `area` either (SPEC §9: no area without t); the cell is
    // still the pointer's.
    assert_eq!(
        events(&fx),
        [(
            "press".into(),
            "".into(),
            json!({"button": 2, "c": 20, "r": 2})
        )]
    );
}

#[test]
fn an_opt_in_outward_of_the_pressed_element_takes_it() {
    let mut h = host();
    show(&mut h, DOC, "0", &[]);
    // The text of <span> inside #m.
    let (taken, fx) = press(&mut h, 20, 0, none());
    assert!(taken);
    assert_eq!(kinds(&fx), ["press m"]);
}

#[test]
fn no_opt_in_shift_alt_and_no_surface_are_the_terminals_and_report_nothing() {
    for p in ["0", "1"] {
        let mut h = host();
        show(&mut h, DOC, p, &[]);
        // No secondary in its data-on, or none at all.
        for (c, r) in [(10, 0), (20, 1), (15, 3), (28, 3)] {
            let (taken, fx) = press(&mut h, c, r, none());
            assert!(!taken, "({c}, {r})");
            assert!(events(&fx).is_empty());
        }
        // Shift, on an element that takes it.
        let (taken, fx) = press(&mut h, 4, 1, with(|m| m.shift = true));
        assert!(!taken);
        assert!(fx.is_empty());
        // Alt: the program's, whatever the data-on says, or Shift with it.
        for m in [
            with(|m| m.alt = true),
            with(|m| {
                m.alt = true;
                m.shift = true;
            }),
        ] {
            let (taken, fx) = press(&mut h, 4, 1, m);
            assert!(!taken);
            assert!(events(&fx).iter().all(|(e, ..)| e != "press"));
        }
        // A surface there is none of.
        assert!(!h.secondary("nope", 5.0, 5.0, none()).taken);
    }
}

#[test]
fn a_pass_through_part_and_a_backdrop_are_not_taken() {
    // Only boxes with `pointer-events: none` at the pointer, the root too.
    let mut h = host();
    show(
        &mut h,
        "<style>html,body{pointer-events:none;margin:0}\
         #s{position:absolute;left:0;top:0;width:90px;height:60px;pointer-events:none}\
         #t{position:absolute;left:100px;top:0;width:30px;height:20px;pointer-events:auto}\
         </style><b id=s data-on=secondary>s</b><i id=t data-on=secondary>t</i>",
        "1",
        &[],
    );
    assert!(!h.takes_pointer("x", 5.0, 5.0));
    let (taken, fx) = press(&mut h, 0, 0, none());
    assert!(!taken);
    assert!(fx.is_empty());
    let (taken, fx) = press(&mut h, 10, 0, none());
    assert!(taken);
    assert_eq!(kinds(&fx), ["press t"]);

    // A backdrop takes no pointer, and what reaches it anyway does nothing.
    let mut h = host();
    show(&mut h, DOC, "1", &[("b", "1")]);
    let (taken, fx) = press(&mut h, 4, 1, none());
    assert!(!taken);
    assert!(fx.is_empty());
    let (taken, fx) = press(&mut h, 4, 1, with(|m| m.alt = true));
    assert!(!taken);
    assert!(fx.is_empty());
}

#[test]
fn alt_leaves_hover_and_gives_the_keyboard_back_as_a_primary_press_with_alt_does() {
    let alt = with(|m| m.alt = true);
    // The primary button's.
    let mut h = host();
    show(&mut h, DOC, "0", &[]);
    h.pointer("x", PointerKind::Move, 45.0, 30.0, none());
    let want = kinds(&h.pointer("x", PointerKind::Down, 45.0, 30.0, alt));
    // The secondary's.
    let mut h = host();
    show(&mut h, DOC, "0", &[]);
    h.pointer("x", PointerKind::Move, 45.0, 30.0, none());
    let (taken, fx) = press(&mut h, 4, 1, alt);
    assert!(!taken);
    assert_eq!(kinds(&fx), want);
    assert_eq!(kinds(&fx), ["hover "]);
    assert_eq!(events(&fx)[0].2, json!({"out": true}));
}

#[test]
fn a_focused_field_keeps_the_keyboard_and_nothing_else_changes() {
    let mut h = host();
    show(&mut h, DOC, "1", &[]);
    // A primary click puts the caret in the field.
    primary(&mut h, PointerKind::Down, 12, 2);
    primary(&mut h, PointerKind::Up, 12, 2);
    assert_eq!(h.focused_surface(), Some("x"));
    // A secondary press elsewhere, on an element that takes it, over the
    // cells beneath it, or the terminal's own: no blur, no focus.
    for (c, r, m) in [
        (4, 1, none()),
        (1, 2, none()),
        (20, 2, none()),
        (4, 1, with(|m| m.shift = true)),
    ] {
        let (_, fx) = press(&mut h, c, r, m);
        assert!(
            events(&fx).iter().all(|(e, ..)| e == "press"),
            "{:?}",
            kinds(&fx)
        );
        assert_eq!(h.focused_surface(), Some("x"));
    }
    // Another surface does not lose the keyboard either.
    h.handle(&cmd(
        &[("a", "doc"), ("s", "y"), ("q", "2")],
        "<input id=g>",
    ));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "y"),
            ("c", "10"),
            ("r", "1"),
            ("q", "2"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    h.pointer("y", PointerKind::Down, 5.0, 5.0, none());
    assert_eq!(h.focused_surface(), Some("y"));
    h.pointer("y", PointerKind::Up, 5.0, 5.0, none());
    let (taken, fx) = press(&mut h, 4, 1, none());
    assert!(taken);
    assert_eq!(kinds(&fx), ["press i"]);
    assert_eq!(h.focused_surface(), Some("y"));
}

#[test]
fn an_element_with_secondary_and_click_gets_no_click() {
    let mut h = host();
    show(&mut h, DOC, "1", &[]);
    let (taken, fx) = press(&mut h, 1, 3, none());
    assert!(taken);
    assert_eq!(kinds(&fx), ["press k"]);
    // The host passes no release, and no move as a drag; the plain move is
    // for hover, and a primary click is a click as before.
    assert!(
        events(&h.pointer("x", PointerKind::Move, 8.0, 70.0, none()))
            .iter()
            .all(|(e, ..)| e == "hover")
    );
    primary(&mut h, PointerKind::Down, 1, 3);
    assert!(kinds(&primary(&mut h, PointerKind::Up, 1, 3)).contains(&"click k".to_string()));
}

#[test]
fn a_secondary_press_starts_no_drag_and_holds_nothing() {
    let mut h = host();
    show(
        &mut h,
        "<style>body{margin:0}b{display:block;width:100px;height:40px}</style>\
         <b id=d data-on=\"drag secondary\">d</b>",
        "1",
        &[],
    );
    let (taken, fx) = press(&mut h, 2, 0, none());
    assert!(taken);
    assert_eq!(kinds(&fx), ["press d"]);
    // The next move is a plain one: no dragstart came, so no drag.
    let fx = h.pointer("x", PointerKind::Move, 150.0, 70.0, none());
    assert!(events(&fx).iter().all(|(e, ..)| e != "drag"));
    // And a primary press still drags.
    let fx = h.pointer("x", PointerKind::Down, 15.0, 10.0, none());
    assert_eq!(kinds(&fx), ["press d", "dragstart d"]);
}

// A long press.

fn long_press(
    h: &mut Host,
    c: u32,
    r: u32,
    mods: Mods,
) -> (Touch, Vec<Effect>, Touch, Vec<Effect>) {
    let (x, y) = (c as f32 * 10.0 + 5.0, r as f32 * 20.0 + 10.0);
    let down = h.touch("x", TouchPhase::Down, x, y, mods);
    assert!(down.effects.is_empty());
    let held = h.touch("x", TouchPhase::LongPress, x, y, mods);
    let up = h.touch("x", TouchPhase::Up, x, y, mods);
    (held.touch, held.effects, up.touch, up.effects)
}

#[test]
fn a_long_press_over_an_opted_in_element_is_a_secondary_press_and_its_lift_no_click() {
    for p in ["0", "1"] {
        let mut h = host();
        show(&mut h, DOC, p, &[]);
        // #k has click as well.
        let (held, fx, up, upfx) = long_press(&mut h, 1, 3, none());
        assert_eq!(held, Touch::Surface);
        assert_eq!(
            events(&fx),
            [(
                "press".into(),
                "k".into(),
                json!({"button": 2, "area": {"c": 0, "r": 3, "w": 3, "h": 1}, "c": 1, "r": 3})
            )]
        );
        assert_eq!(up, Touch::Surface);
        assert!(upfx.is_empty());
        // Nested: the innermost.
        let (held, fx, ..) = long_press(&mut h, 4, 1, none());
        assert_eq!(held, Touch::Surface);
        assert_eq!(kinds(&fx), ["press i"]);
    }
}

#[test]
fn a_long_press_elsewhere_is_the_terminals_and_so_is_one_with_shift_or_alt() {
    let mut h = host();
    show(&mut h, DOC, "1", &[]);
    for (c, r, m) in [
        (10, 0, none()),
        (20, 1, none()),
        (4, 1, with(|m| m.shift = true)),
        (4, 1, with(|m| m.alt = true)),
    ] {
        let (held, fx, up, upfx) = long_press(&mut h, c, r, m);
        assert_eq!(held, Touch::Terminal, "({c}, {r})");
        assert!(fx.is_empty());
        // A long press never taps.
        assert_eq!(up, Touch::Terminal);
        assert!(upfx.is_empty());
    }
}

#[test]
fn nothing_else_of_a_taken_long_press_is_reported() {
    let mut h = host();
    show(&mut h, DOC, "1", &[]);
    let (x, y) = (15.0, 70.0);
    h.touch("x", TouchPhase::Down, x, y, none());
    let held = h.touch("x", TouchPhase::LongPress, x, y, none());
    assert_eq!(kinds(&held.effects), ["press k"]);
    // A second long press, and a move, report nothing and stay the surface's.
    let again = h.touch("x", TouchPhase::LongPress, x, y, none());
    assert!(again.effects.is_empty());
    let moved = h.touch("x", TouchPhase::Move, x + 40.0, y, none());
    assert_eq!((moved.touch, moved.effects.len()), (Touch::Surface, 0));
    let up = h.touch("x", TouchPhase::Up, x + 40.0, y, none());
    assert_eq!((up.touch, up.effects.len()), (Touch::Surface, 0));
    // The next touch is a tap as before: a click.
    h.touch("x", TouchPhase::Down, x, y, none());
    let up = h.touch("x", TouchPhase::Up, x, y, none());
    assert_eq!(up.touch, Touch::Surface);
    assert!(kinds(&up.effects).contains(&"click k".to_string()));
}

#[test]
fn a_backdrop_takes_no_long_press() {
    let mut h = host();
    show(&mut h, DOC, "1", &[("b", "1")]);
    let (held, fx, ..) = long_press(&mut h, 4, 1, none());
    assert_eq!(held, Touch::Terminal);
    assert!(fx.is_empty());
}

//! The versions the host speaks (SPEC §4, §15; HOST.md §4): 0.7 and 0.6, a
//! query answered in the highest it shares, and a 0.6 session, which has no
//! secondary press (HOST.md §4.1). The shared vectors cover the common path;
//! these the corners.

mod common;

use common::body;
use hotty::Message;
use hotty::host::{Output, Splitter};
use hotty_blitz::{Config, Effect, Host, Metrics, Mods, PointerKind, Touch, TouchPhase};
use serde_json::{Value, json};

fn host() -> Host {
    Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    })
}

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// The replies among effects: their control's `a` and the body.
fn replies(effects: &[Effect]) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    let mut s = Splitter::with_max(hotty_blitz::MAX_COMMAND_BYTES);
    for e in effects {
        if let Effect::Reply(b) = e {
            s.feed(b, &mut |o| {
                if let Output::Command(c) = o {
                    out.push((c.get("a").unwrap_or("").to_string(), body(&c.payload)));
                }
            });
        }
    }
    out
}

/// What a query of `v` is answered with: `ok` and the capabilities' `v`, or
/// `err` and the error's code and detail.
fn ask(h: &mut Host, v: &str) -> (String, Value) {
    let fx = h.handle(&cmd(&[("a", "q"), ("n", "1"), ("v", v)], ""));
    let mut r = replies(&fx);
    assert_eq!(r.len(), 1, "{v}");
    let (a, b) = r.remove(0);
    match a.as_str() {
        "ok" => (a, b["v"].clone()),
        _ => (a, json!([b["code"], b["detail"]])),
    }
}

#[test]
fn a_query_is_answered_in_the_highest_version_it_shares() {
    for (v, want) in [
        ("0.6,0.7", "0.7"),
        ("0.7,0.6", "0.7"),
        ("0.7", "0.7"),
        ("0.6", "0.6"),
        ("0.5,0.6", "0.6"),
        ("0.6,99.0", "0.6"),
        ("0.5,0.7,1.0", "0.7"),
    ] {
        let mut h = host();
        assert_eq!(ask(&mut h, v), ("ok".into(), json!(want)), "{v}");
    }
}

#[test]
fn a_query_that_shares_no_version_is_eversion_with_the_versions_spoken_lowest_first() {
    for v in ["0.5", "0.8", "1.0", "", "0.60,0.70"] {
        let mut h = host();
        assert_eq!(
            ask(&mut h, v),
            ("err".into(), json!(["EVERSION", "0.6,0.7"])),
            "{v:?}"
        );
    }
    // No `v` at all.
    let mut h = host();
    let fx = h.handle(&cmd(&[("a", "q"), ("n", "1")], ""));
    let r = replies(&fx);
    assert_eq!(r[0].0, "err");
    assert_eq!(r[0].1["detail"], "0.6,0.7");
}

#[test]
fn the_host_names_the_highest_version_it_speaks() {
    assert_eq!(hotty_blitz::VERSION, "0.7");
    assert_eq!(hotty_blitz::VERSION, hotty::VERSION);
    assert_eq!(hotty::VERSIONS, ["0.7", "0.6"]);
}

// A 0.6 session.

/// Boxes in cells of 10×20 px, on a surface of 30×4 cells.
const DOC: &str = "<style>body{margin:0}\
    b,i,u,input{position:absolute;display:block;box-sizing:border-box;margin:0;padding:0;\
    font-style:normal;font-weight:normal;text-decoration:none;height:20px}\
    #o{left:0;top:0;width:90px;height:60px}\
    #i{left:30px;top:20px;width:30px}\
    #n{left:100px;top:0;width:30px}\
    #f{left:100px;top:40px;width:60px}</style>\
    <b id=o data-on=secondary><u id=i data-on=\"secondary\">i</u></b>\
    <i id=n>n</i><input id=f type=text>";

fn session(v: &str) -> Host {
    let mut h = host();
    h.set_backdrops(true);
    assert_eq!(ask(&mut h, v).0, "ok");
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], DOC));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "30"),
            ("r", "4"),
            ("p", "1"),
            ("v", "1"),
            ("q", "2"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    h
}

fn none() -> Mods {
    Mods::default()
}

fn alt() -> Mods {
    Mods {
        alt: true,
        ..Mods::default()
    }
}

/// The secondary press at the centre of cell (`c`, `r`).
fn press(h: &mut Host, c: u32, r: u32, mods: Mods) -> (bool, Vec<Effect>) {
    let out = h.secondary("x", c as f32 * 10.0 + 5.0, r as f32 * 20.0 + 10.0, mods);
    (out.taken, out.effects)
}

#[test]
fn a_zero_point_six_session_takes_no_secondary_press_and_leaves_the_surface_as_it_was() {
    let mut h = session("0.6");
    // The caret in the field, then the pointer over #i.
    h.pointer("x", PointerKind::Down, 115.0, 50.0, none());
    h.pointer("x", PointerKind::Up, 115.0, 50.0, none());
    assert_eq!(h.focused_surface(), Some("x"));
    h.pointer("x", PointerKind::Move, 45.0, 30.0, none());
    for m in [none(), alt()] {
        let (taken, fx) = press(&mut h, 4, 1, m);
        assert!(!taken);
        // No press, and no hover out with Alt.
        assert!(fx.is_empty(), "{:?}", replies(&fx));
        assert_eq!(h.focused_surface(), Some("x"));
    }
    // The pointer is still over #i: leaving the surface ends the hover.
    let fx = h.pointer("x", PointerKind::Leave, 45.0, 30.0, none());
    assert!(
        replies(&fx).iter().any(|(_, b)| b == &json!({"out": true})),
        "{:?}",
        replies(&fx)
    );
}

#[test]
fn a_zero_point_seven_session_is_as_it_was() {
    for v in ["0.7", "0.6,0.7"] {
        let mut h = session(v);
        h.pointer("x", PointerKind::Move, 45.0, 30.0, none());
        let (taken, fx) = press(&mut h, 4, 1, none());
        assert!(taken, "{v}");
        assert_eq!(replies(&fx).len(), 1);
        // With Alt: not taken, hover out, the program's.
        let (taken, fx) = press(&mut h, 4, 1, alt());
        assert!(!taken);
        assert_eq!(replies(&fx)[0].1, json!({"out": true}));
    }
    // A session before any query is in the highest version.
    let mut h = host();
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], DOC));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "30"),
            ("r", "4"),
            ("p", "1"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    assert!(press(&mut h, 4, 1, none()).0);
}

#[test]
fn the_next_query_changes_the_session_and_a_reset_ends_it() {
    let mut h = session("0.6");
    assert!(!press(&mut h, 4, 1, none()).0);
    assert_eq!(ask(&mut h, "0.6,0.7").0, "ok");
    assert!(press(&mut h, 4, 1, none()).0);
    assert_eq!(ask(&mut h, "0.6").0, "ok");
    assert!(!press(&mut h, 4, 1, none()).0);
    // A query no version of which is shared leaves the session as it was.
    assert_eq!(ask(&mut h, "0.5").0, "err");
    // RIS, or the end of the program's output: the next program asks again,
    // and until it does the session is in the highest version.
    h.reset();
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], DOC));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "30"),
            ("r", "4"),
            ("p", "1"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    assert!(press(&mut h, 4, 1, none()).0);
}

fn long_press(h: &mut Host, c: u32, r: u32, mods: Mods) -> (Touch, usize, Touch, usize) {
    let (x, y) = (c as f32 * 10.0 + 5.0, r as f32 * 20.0 + 10.0);
    h.touch("x", TouchPhase::Down, x, y, mods);
    let held = h.touch("x", TouchPhase::LongPress, x, y, mods);
    let up = h.touch("x", TouchPhase::Up, x, y, mods);
    (held.touch, held.effects.len(), up.touch, up.effects.len())
}

#[test]
fn a_long_press_in_a_zero_point_six_session_is_no_secondary_press() {
    // As in 0.6.9: the terminal's, and the lift is no tap.
    let mut h = session("0.6");
    for (c, r) in [(4, 1), (1, 0)] {
        assert_eq!(
            long_press(&mut h, c, r, none()),
            (Touch::Terminal, 0, Touch::Terminal, 0),
            "({c}, {r})"
        );
    }
    // In 0.7 it is one.
    let mut h = session("0.7");
    assert_eq!(
        long_press(&mut h, 4, 1, none()),
        (Touch::Surface, 1, Touch::Surface, 0)
    );
}

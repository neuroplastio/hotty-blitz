//! `view` (SPEC §9.5) and `reveal` (§6.1): what the shared vectors do not
//! reach. A scroll that moves holds `view` back until it comes to rest, and
//! what is clipped, hidden or detached is not reported.

mod common;

use common::{body, delta_cmd};
use hotty_blitz::{Config, Effect, Host, Key, KeyName, Metrics, Mods, PointerKind};
use hotty_wire::{Command, Event, Scanner};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

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

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Command {
    Command::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// Every HOTTY event in the effects, as (`e`, `t`, detail).
fn events(effects: &[Effect]) -> Vec<(String, String, Value)> {
    let mut out = Vec::new();
    let mut s = Scanner::new();
    for e in effects {
        if let Effect::Reply(b) = e {
            s.feed(b, &mut |ev| {
                if let Event::Command(c) = ev
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

/// The `view`s a frame drawn now tells the program, as their details.
fn views(h: &mut Host) -> Vec<Value> {
    h.render_dirty(&mut |_, _, _| {});
    events(&h.take_events())
        .into_iter()
        .map(|(e, t, detail)| {
            assert_eq!((e.as_str(), t.as_str()), ("view", ""));
            detail
        })
        .collect()
}

/// A `view` detail: each id in view at row `r`, a row tall and four
/// columns wide, from the first column.
fn rows(at: &[(&str, i64)], end: bool) -> Value {
    let inside: serde_json::Map<String, Value> = at
        .iter()
        .map(|&(id, r)| (id.to_string(), json!({"c": 0, "r": r, "w": 4, "h": 1})))
        .collect();
    json!({"in": inside, "end": end})
}

/// Eight rows of 20 px, each asking for `view`.
const ROWS: &str = "<style>body{margin:0}div{height:20px;width:40px}</style>\
    <div id=a data-on=view>a</div><div id=b data-on=view>b</div>\
    <div id=c data-on=view>c</div><div id=d data-on=view>d</div>\
    <div id=e data-on=view>e</div><div id=f data-on=view>f</div>\
    <div id=g data-on=view>g</div><div id=h data-on=view>h</div>";

/// A document (`scroll` 1, or none) placed 20 × 4, and what it hears first.
fn place(h: &mut Host, scroll: bool, html: &str) -> Vec<Value> {
    let mut pairs = vec![("a", "doc"), ("s", "x"), ("q", "2")];
    if scroll {
        pairs.push(("scroll", "1"));
    }
    h.handle(&cmd(&pairs, html));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "20"),
            ("r", "4"),
            ("q", "2"),
        ],
        "",
    ));
    views(h)
}

fn delta(h: &mut Host, op: &str, t: &str, payload: &str) -> Vec<Effect> {
    h.handle(&delta_cmd(&[("s", "x"), ("q", "2")], op, Some(t), None, payload))
}

#[test]
fn a_wheel_gesture_holds_view_back_until_it_comes_to_rest() {
    let mut h = host();
    let first = place(&mut h, true, ROWS);
    assert_eq!(
        first,
        [rows(&[("a", 0), ("b", 1), ("c", 2), ("d", 3)], false)]
    );
    // Two wheels of one gesture: the frames they draw tell nothing.
    for _ in 0..2 {
        assert!(h.wheel("x", 15.0, 10.0, 0.0, 20.0, Mods::default()).taken);
        assert_eq!(views(&mut h), Vec::<Value>::new(), "still moving");
    }
    assert!(!h.has_dirty(), "nothing to draw while it moves");
    let rest = h.next_frame().expect("a frame once it rests");
    assert!(rest > Instant::now(), "not yet");
    // The gesture ends (a touchpad's fingers lift): one view, now.
    h.end_gesture();
    assert!(h.has_dirty());
    assert!(h.next_frame().is_some_and(|t| t <= Instant::now()));
    assert_eq!(
        views(&mut h),
        [rows(&[("c", 0), ("d", 1), ("e", 2), ("f", 3)], false)]
    );
    assert_eq!(views(&mut h), Vec::<Value>::new(), "heard");
    assert!(!h.has_dirty());
    // A wheel's notches end on their own: 150 ms after the last.
    assert!(h.wheel("x", 15.0, 10.0, 0.0, 1000.0, Mods::default()).taken);
    assert_eq!(views(&mut h), Vec::<Value>::new());
    std::thread::sleep(Duration::from_millis(160));
    assert!(h.has_dirty(), "at rest");
    assert_eq!(
        views(&mut h),
        [rows(&[("e", 0), ("f", 1), ("g", 2), ("h", 3)], true)]
    );
}

#[test]
fn a_key_that_scrolls_tells_view_with_the_next_frame() {
    let mut h = host();
    place(&mut h, true, ROWS);
    h.handle(&cmd(&[("a", "focus"), ("s", "x"), ("q", "2")], ""));
    let end = Key {
        name: KeyName::End,
        names: Vec::new(),
        mods: Mods::default(),
    };
    assert!(h.key("x", &end).consumed);
    assert_eq!(
        views(&mut h),
        [rows(&[("e", 0), ("f", 1), ("g", 2), ("h", 3)], true)]
    );
}

#[test]
fn a_thumb_held_holds_view_back_until_its_release() {
    let mut h = host();
    place(&mut h, true, ROWS);
    // A wheel shows the root's thumb: 80 px of 160, at x 188 to 198.
    assert!(h.wheel("x", 15.0, 10.0, 0.0, 10.0, Mods::default()).taken);
    h.end_gesture();
    assert_eq!(views(&mut h).len(), 1);
    let mut fx = h.pointer("x", PointerKind::Down, 193.0, 20.0, Mods::default());
    fx.extend(h.pointer("x", PointerKind::Move, 193.0, 80.0, Mods::default()));
    assert_eq!(events(&fx), []);
    assert_eq!(views(&mut h), Vec::<Value>::new(), "held");
    assert!(!h.has_dirty());
    h.pointer("x", PointerKind::Up, 193.0, 80.0, Mods::default());
    assert_eq!(
        views(&mut h),
        [rows(&[("e", 0), ("f", 1), ("g", 2), ("h", 3)], true)]
    );
}

#[test]
fn what_a_box_clips_hides_or_does_not_box_is_not_in_view() {
    let mut h = host();
    let first = place(
        &mut h,
        false,
        "<style>body{margin:0}div{height:20px;width:40px}</style>\
         <div id=a data-on=view>a</div>\
         <div id=clip style='overflow:hidden'>\
           <div id=in1 data-on=view>in1</div><div id=in2 data-on=view>in2</div>\
           <div id=out style='position:absolute;left:100px;top:40px' data-on=view>out</div>\
         </div>\
         <div id=hid data-on=view style='visibility:hidden'>h</div>\
         <div data-on=view>no id</div>\
         <section id=dc data-on=view style='display:contents'><div>dc</div></section>",
    );
    // in2 lies in the window, under a box that clips it; `out` escapes it,
    // positioned by the root. A hidden element, one with no id and one
    // with no box of its own are not reported.
    assert_eq!(
        first,
        [json!({"in": {
            "a": {"c": 0, "r": 0, "w": 4, "h": 1},
            "in1": {"c": 0, "r": 1, "w": 4, "h": 1},
            "out": {"c": 10, "r": 2, "w": 4, "h": 1},
        }, "end": true})]
    );
    // The same placement again: what it carries is what the program last
    // heard, so it sends nothing (SPEC §9.5).
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "20"),
            ("r", "4"),
            ("q", "2"),
        ],
        "",
    ));
    assert_eq!(views(&mut h), Vec::<Value>::new());
}

#[test]
fn an_element_that_asks_without_an_id_hears_only_end() {
    let mut h = host();
    let page = ROWS
        .replace(" data-on=view", "")
        .replace("<div id=a>", "<div id=a data-on=view hidden>");
    let page = format!("<body data-on=view>{page}</body>");
    assert_eq!(
        place(&mut h, true, &page),
        [json!({"in": {}, "end": false})]
    );
    delta(&mut h, "reveal", "h", "");
    assert_eq!(views(&mut h), [json!({"in": {}, "end": true})]);
}

#[test]
fn a_detached_surface_hears_no_view_until_a_document_of_its_own() {
    let mut h = host();
    place(&mut h, false, ROWS);
    h.handle(&cmd(&[("a", "detach"), ("s", "x"), ("q", "2")], ""));
    delta(&mut h, "remove", "a", "");
    assert_eq!(views(&mut h), Vec::<Value>::new(), "detached");
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], ROWS));
    assert_eq!(
        views(&mut h),
        [rows(&[("a", 0), ("b", 1), ("c", 2), ("d", 3)], true)]
    );
}

#[test]
fn reveal_moves_no_focus_and_scrolls_only_what_scrolls() {
    let mut h = host();
    // A field, a box one row tall that clips without scrolling, and the
    // eight rows: ten rows of 20 px in four.
    place(
        &mut h,
        true,
        &format!(
            "<input id=field style='display:block;height:20px;margin:0;padding:0;border:0'>\
             <div id=clip style='height:20px;overflow:hidden'>\
               <div id=c1 style='height:20px'>c1</div>\
               <div id=c2 data-on=view style='height:20px;width:40px'>c2</div>\
             </div>{ROWS}"
        ),
    );
    h.handle(&cmd(
        &[("a", "focus"), ("s", "x"), ("t", "field"), ("q", "2")],
        "",
    ));
    // As far as the root goes towards h's top: the field keeps focus and
    // the keyboard, and nothing is said but what is in view.
    assert_eq!(events(&delta(&mut h, "reveal", "h", "start")), []);
    assert_eq!(
        views(&mut h),
        [rows(&[("e", 0), ("f", 1), ("g", 2), ("h", 3)], true)]
    );
    assert!(h.is_focused("x"));
    assert_ne!(h.computed_style("x", "#field:focus", "display", false), "");
    // A box that clips without scrolling (`overflow: hidden`) stays put:
    // the root brings c2's border box into view, and the box still clips
    // it.
    delta(&mut h, "reveal", "c2", "");
    assert_eq!(
        views(&mut h),
        [rows(&[("a", 0), ("b", 1), ("c", 2), ("d", 3)], false)]
    );
    // An element with no box moves nothing.
    h.handle(&delta_cmd(&[("s", "x"), ("q", "2")], "attr", Some("e"), Some("hidden"), ""));
    assert_eq!(views(&mut h), Vec::<Value>::new());
    delta(&mut h, "reveal", "e", "start");
    assert_eq!(views(&mut h), Vec::<Value>::new(), "e is not rendered");
}

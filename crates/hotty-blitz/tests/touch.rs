//! Touches (SPEC §9.1): which drag an element and which pan, what a touch
//! that drags reports, and what ends one. The shared vectors cover the
//! common path; these the corners.

use hotty_blitz::{Config, Effect, Host, Metrics, Mods, Touch, TouchPhase};
use hotty_wire::{Command, Event, Scanner};

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

/// A document with `scroll` (or none), placed 30×4 with presses reported,
/// and drawn.
fn place(h: &mut Host, scroll: Option<&str>, body: &str) {
    let html = format!("<style>body{{margin:0}}div{{height:20px;width:200px}}</style>{body}");
    let mut pairs = vec![("a", "doc"), ("s", "x"), ("q", "2")];
    if let Some(v) = scroll {
        pairs.push(("scroll", v));
    }
    h.handle(&cmd(&pairs, &html));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "30"),
            ("r", "4"),
            ("p", "1"),
            ("q", "2"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
}

/// The events among effects, as `kind target`.
fn events(effects: &[Effect]) -> Vec<String> {
    let mut out = Vec::new();
    let mut s = Scanner::new();
    for e in effects {
        if let Effect::Reply(b) = e {
            s.feed(b, &mut |ev| {
                if let Event::Command(c) = ev
                    && c.get("a") == Some("ev")
                {
                    out.push(format!(
                        "{} {}",
                        c.get("e").unwrap_or(""),
                        c.get("t").unwrap_or("")
                    ));
                }
            });
        }
    }
    out
}

fn touch(h: &mut Host, phase: TouchPhase, x: f32, y: f32, mods: Mods) -> (Touch, Vec<String>) {
    let out = h.touch("x", phase, x, y, mods);
    (out.touch, events(&out.effects))
}

/// A touch at (15, 10), on the first row, that moves by (`dx`, `dy`) and
/// lifts: whose it was once it moved, and every event it made.
fn swipe(h: &mut Host, dx: f32, dy: f32) -> (Touch, Vec<String>) {
    let none = Mods::default();
    let (_, mut all) = touch(h, TouchPhase::Down, 15.0, 10.0, none);
    let (moved, ev) = touch(h, TouchPhase::Move, 15.0 + dx, 10.0 + dy, none);
    all.extend(ev);
    all.extend(touch(h, TouchPhase::Up, 15.0 + dx, 10.0 + dy, none).1);
    (moved, all)
}

#[test]
fn touch_action_decides_which_way_a_touch_drags() {
    // (value, drags sideways, drags up and down)
    for (value, x, y) in [
        ("none", true, true),
        ("pan-x", false, true),
        ("pan-y", true, false),
        ("auto", false, false),
        ("manipulation", false, false),
        ("pinch-zoom", true, true),
        ("pan-x pinch-zoom", false, true),
    ] {
        let body = format!(
            "<div id=d data-on=drag style=\"touch-action:{value}\">d</div><div id=o>o</div>"
        );
        for (dx, dy, drags) in [(30.0, 0.0, x), (0.0, 30.0, y)] {
            let mut h = host();
            place(&mut h, None, &body);
            let down = h
                .touch("x", TouchPhase::Down, 15.0, 10.0, Mods::default())
                .touch;
            assert_eq!(
                down,
                if x || y {
                    Touch::Undecided
                } else {
                    Touch::Terminal
                },
                "{value}: down"
            );
            let (moved, ev) = swipe(&mut h, dx, dy);
            if drags {
                assert_eq!(moved, Touch::Surface, "{value} by ({dx}, {dy})");
                assert_eq!(&ev[..2], ["press d", "dragstart d"], "{value}: {ev:?}");
                assert!(ev.last().unwrap().starts_with("dragend"), "{value}: {ev:?}");
            } else {
                assert_eq!(moved, Touch::Terminal, "{value} by ({dx}, {dy})");
                assert!(ev.is_empty(), "{value}: a pan presses nothing: {ev:?}");
            }
        }
    }
}

#[test]
fn a_tie_pans_and_a_move_within_the_slop_decides_nothing() {
    let mut h = host();
    place(
        &mut h,
        None,
        "<div id=d data-on=drag style=\"touch-action:none\">d</div>",
    );
    let (moved, ev) = swipe(&mut h, 20.0, 20.0);
    assert_eq!(moved, Touch::Terminal);
    assert!(ev.is_empty(), "{ev:?}");

    // 7 CSS px is within the 8 px slop: still undecided, and lifting there
    // is a tap, a click with no drag.
    let none = Mods::default();
    touch(&mut h, TouchPhase::Down, 15.0, 10.0, none);
    assert_eq!(
        touch(&mut h, TouchPhase::Move, 22.0, 10.0, none).0,
        Touch::Undecided
    );
    let (up, ev) = touch(&mut h, TouchPhase::Up, 22.0, 10.0, none);
    assert_eq!(up, Touch::Surface);
    assert_eq!(
        ev,
        ["press d"],
        "a tap on a drag element presses, and drags nothing"
    );
}

#[test]
fn the_touched_elements_value_counts_with_its_ancestors() {
    // The element that opts in allows a vertical pan; the part touched
    // allows none, so a vertical touch on it drags.
    let mut h = host();
    place(
        &mut h,
        None,
        "<div id=d data-on=drag style=\"touch-action:pan-y;height:40px\">\
         <div id=k style=\"touch-action:none\">k</div></div>",
    );
    let (moved, ev) = swipe(&mut h, 0.0, 30.0);
    assert_eq!(moved, Touch::Surface);
    assert_eq!(&ev[..2], ["press k", "dragstart d"]);

    // An ancestor's pan-y keeps a sideways touch on an `auto` element from
    // panning.
    let mut h = host();
    place(
        &mut h,
        None,
        "<div style=\"touch-action:pan-y;height:40px\"><div id=d data-on=drag>d</div></div>",
    );
    assert_eq!(swipe(&mut h, 30.0, 0.0).0, Touch::Surface);
    assert_eq!(swipe(&mut h, 0.0, 30.0).0, Touch::Terminal);
}

#[test]
fn the_values_count_up_to_the_nearest_element_that_scrolls() {
    // Inside a box that scrolls, the drag element's `none` beyond it does
    // not count: the touch pans.
    let mut h = host();
    place(
        &mut h,
        Some("1"),
        "<div id=d data-on=drag style=\"touch-action:none;height:60px\">\
         <div style=\"overflow:auto;height:40px\"><div id=in style=\"height:80px\">in</div></div>\
         </div>",
    );
    assert_eq!(swipe(&mut h, 30.0, 0.0).0, Touch::Terminal);
    // On the drag element itself, outside the box, it drags.
    let none = Mods::default();
    touch(&mut h, TouchPhase::Down, 15.0, 50.0, none);
    assert_eq!(
        touch(&mut h, TouchPhase::Move, 45.0, 50.0, none).0,
        Touch::Surface
    );
}

#[test]
fn touch_action_elsewhere_changes_nothing() {
    let mut h = host();
    place(
        &mut h,
        None,
        "<div id=n style=\"touch-action:none\">n</div>",
    );
    let none = Mods::default();
    assert_eq!(
        touch(&mut h, TouchPhase::Down, 15.0, 10.0, none).0,
        Touch::Terminal
    );
    let (moved, ev) = touch(&mut h, TouchPhase::Move, 45.0, 10.0, none);
    assert_eq!((moved, ev.len()), (Touch::Terminal, 0));
    // After a pan, the lift is the terminal's: no tap.
    assert_eq!(
        touch(&mut h, TouchPhase::Up, 45.0, 10.0, none),
        (Touch::Terminal, vec![])
    );
}

#[test]
fn a_drag_reports_where_the_finger_already_is() {
    let mut h = host();
    place(
        &mut h,
        None,
        "<div id=d data-on=drag style=\"touch-action:none\">d</div><div id=o data-on=drag>o</div>",
    );
    let (moved, ev) = swipe(&mut h, 0.0, 20.0);
    assert_eq!(moved, Touch::Surface);
    assert_eq!(ev, ["press d", "dragstart d", "drag o", "dragend o"]);
}

#[test]
fn a_second_finger_ends_the_drag_and_the_rest_is_the_terminals() {
    let mut h = host();
    place(
        &mut h,
        None,
        "<div id=d data-on=\"drag click\" style=\"touch-action:none\">d</div>",
    );
    let none = Mods::default();
    touch(&mut h, TouchPhase::Down, 15.0, 10.0, none);
    assert_eq!(
        touch(&mut h, TouchPhase::Move, 45.0, 10.0, none).0,
        Touch::Surface
    );
    let (after, ev) = touch(&mut h, TouchPhase::Cancel, 45.0, 10.0, none);
    assert_eq!((after, ev), (Touch::Terminal, vec!["dragend ".to_string()]));
    assert_eq!(
        touch(&mut h, TouchPhase::Move, 60.0, 10.0, none),
        (Touch::Terminal, vec![])
    );
    // No click at the lift, though it is where the drag began.
    assert_eq!(
        touch(&mut h, TouchPhase::Up, 20.0, 10.0, none),
        (Touch::Terminal, vec![])
    );

    // A new touch while one still drags (its lift lost) ends that drag.
    touch(&mut h, TouchPhase::Down, 15.0, 10.0, none);
    touch(&mut h, TouchPhase::Move, 45.0, 10.0, none);
    let (_, ev) = touch(&mut h, TouchPhase::Down, 15.0, 10.0, none);
    assert_eq!(ev, ["dragend "]);
}

#[test]
fn a_touch_with_alt_held_never_drags_and_its_tap_still_clicks() {
    let mut h = host();
    place(
        &mut h,
        None,
        "<div id=d data-on=\"drag click\" style=\"touch-action:none\">d</div>",
    );
    let alt = Mods {
        alt: true,
        ..Mods::default()
    };
    assert_eq!(
        touch(&mut h, TouchPhase::Down, 15.0, 10.0, alt).0,
        Touch::Terminal
    );
    assert_eq!(
        touch(&mut h, TouchPhase::Move, 45.0, 10.0, alt),
        (Touch::Terminal, vec![])
    );
    assert_eq!(
        touch(&mut h, TouchPhase::Up, 45.0, 10.0, alt),
        (Touch::Terminal, vec![])
    );

    touch(&mut h, TouchPhase::Down, 15.0, 10.0, alt);
    let (up, ev) = touch(&mut h, TouchPhase::Up, 15.0, 10.0, alt);
    assert_eq!(up, Touch::Surface);
    assert_eq!(ev, ["press d", "click d"]);
}

#[test]
fn a_long_press_never_drags_nor_taps() {
    let mut h = host();
    place(
        &mut h,
        None,
        "<div id=d data-on=\"drag click\" style=\"touch-action:none\">d</div>",
    );
    let none = Mods::default();
    assert_eq!(
        touch(&mut h, TouchPhase::Down, 15.0, 10.0, none).0,
        Touch::Undecided
    );
    assert_eq!(
        touch(&mut h, TouchPhase::LongPress, 15.0, 10.0, none),
        (Touch::Terminal, vec![])
    );
    assert_eq!(
        touch(&mut h, TouchPhase::Move, 45.0, 10.0, none),
        (Touch::Terminal, vec![])
    );
    assert_eq!(
        touch(&mut h, TouchPhase::Up, 15.0, 10.0, none),
        (Touch::Terminal, vec![])
    );
}

#[test]
fn a_drag_is_a_click_only_where_it_began() {
    let mut h = host();
    place(
        &mut h,
        None,
        "<div id=d data-on=\"drag click\" style=\"touch-action:pan-y\">d</div><div id=o>o</div>",
    );
    let none = Mods::default();
    touch(&mut h, TouchPhase::Down, 15.0, 10.0, none);
    touch(&mut h, TouchPhase::Move, 45.0, 10.0, none);
    touch(&mut h, TouchPhase::Move, 45.0, 30.0, none);
    let (up, ev) = touch(&mut h, TouchPhase::Up, 45.0, 30.0, none);
    assert_eq!((up, ev), (Touch::Surface, vec!["dragend ".to_string()]));

    touch(&mut h, TouchPhase::Down, 15.0, 10.0, none);
    touch(&mut h, TouchPhase::Move, 45.0, 10.0, none);
    let (_, ev) = touch(&mut h, TouchPhase::Up, 60.0, 10.0, none);
    assert_eq!(ev, ["dragend d", "click d"]);
}

#[test]
fn a_touch_over_no_surface_or_a_detached_one_is_the_terminals() {
    let mut h = host();
    place(
        &mut h,
        None,
        "<div id=d data-on=drag style=\"touch-action:none\">d</div>",
    );
    let none = Mods::default();
    assert_eq!(
        h.touch("nope", TouchPhase::Down, 15.0, 10.0, none).touch,
        Touch::Terminal
    );
    assert_eq!(
        h.touch("nope", TouchPhase::Up, 15.0, 10.0, none).touch,
        Touch::Terminal
    );
    h.handle(&cmd(&[("a", "detach"), ("s", "x"), ("q", "2")], ""));
    assert_eq!(
        touch(&mut h, TouchPhase::Down, 15.0, 10.0, none).0,
        Touch::Terminal
    );
    assert_eq!(
        touch(&mut h, TouchPhase::Move, 45.0, 10.0, none),
        (Touch::Terminal, vec![])
    );
}

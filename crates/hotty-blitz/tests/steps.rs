//! Steps (SPEC §9.1): where in a dragged element with `data-steps` the
//! pointer is. The shared vectors point at cell centres; these cover the
//! value's parsing, the rounding, the clamping, a touch, and what a delta
//! does to a drag under way.

mod common;

use common::body;
use hotty_blitz::{Config, Effect, Host, Metrics, Mods, PointerKind, TouchPhase};
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

/// `body` in a surface of 30×4 cells of 10×20 pixels, its divs 200×20,
/// placed and drawn.
fn place(h: &mut Host, body: &str) {
    let html = format!("<style>body{{margin:0}}div{{height:20px;width:200px}}</style>{body}");
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], &html));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "30"), ("r", "4"), ("q", "2")],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
}

/// A drag event: its kind, its target, and the steps in its detail.
type Drag = (String, String, Option<u64>, Option<u64>);

fn drags(effects: &[Effect]) -> Vec<Drag> {
    let mut out = Vec::new();
    let mut s = Scanner::new();
    for e in effects {
        if let Effect::Reply(b) = e {
            s.feed(b, &mut |ev| {
                if let Event::Command(c) = ev
                    && c.get("a") == Some("ev")
                    && c.get("e").is_some_and(|k| k.starts_with("drag"))
                {
                    let d = body(&c.payload);
                    out.push((
                        c.get("e").unwrap().to_string(),
                        c.get("t").unwrap_or("").to_string(),
                        d["x"].as_u64(),
                        d["y"].as_u64(),
                    ));
                }
            });
        }
    }
    out
}

fn ev(kind: &str, target: &str, x: Option<u64>, y: Option<u64>) -> Drag {
    (kind.to_string(), target.to_string(), x, y)
}

/// The mouse at device pixel (`x`, `y`) of the surface.
fn at(h: &mut Host, kind: PointerKind, x: f32, y: f32) -> Vec<Drag> {
    drags(&h.pointer("x", kind, x, y, Mods::default()))
}

#[test]
fn data_steps_is_one_or_two_whole_numbers_and_the_host_says_it_reads_them() {
    let caps = host().handle(&cmd(&[("a", "q"), ("n", "1")], ""));
    let Some(Effect::Reply(b)) = caps.first() else {
        panic!("no reply to q");
    };
    let mut steps = None;
    Scanner::new().feed(b, &mut |e| {
        if let Event::Command(c) = e {
            steps = Some(body(&c.payload)["steps"].clone());
        }
    });
    assert_eq!(steps, Some(serde_json::Value::Bool(true)));

    // At the middle of the box: half of each count.
    for (value, want) in [
        ("4", (Some(2), None)),
        ("0 4", (None, Some(2))),
        (" 4\t 2 ", (Some(2), Some(1))),
        ("0", (None, None)),
        ("0 0", (None, None)),
        ("2.5", (None, None)),
        ("-1", (None, None)),
        ("+3", (None, None)),
        ("1e2", (None, None)),
        ("4 2 1", (None, None)),
        ("four", (None, None)),
        ("", (None, None)),
        // The most a body's int holds (SPEC §3.3), and one past it.
        ("9007199254740991", (Some(4503599627370496), None)),
        ("9007199254740992", (None, None)),
    ] {
        let mut h = host();
        place(&mut h, &format!("<div id=t data-on=drag data-steps=\"{value}\">t</div>"));
        let got = at(&mut h, PointerKind::Down, 100.0, 10.0);
        assert_eq!(got, [ev("dragstart", "t", want.0, want.1)], "data-steps={value:?}");
        at(&mut h, PointerKind::Up, 100.0, 10.0);
    }
}

#[test]
fn a_step_rounds_to_the_nearest_a_half_up_and_a_change_of_step_alone_is_a_drag() {
    let mut h = host();
    // 25 pixels a step.
    place(&mut h, "<div id=t data-on=drag data-steps=8>t</div>");
    let mut got = at(&mut h, PointerKind::Down, 1.0, 10.0);
    for x in [12.0, 12.5, 37.0, 37.5, 199.0] {
        got.extend(at(&mut h, PointerKind::Move, x, 10.0));
    }
    got.extend(at(&mut h, PointerKind::Up, 199.0, 10.0));
    assert_eq!(
        got,
        [
            ev("dragstart", "t", Some(0), None),
            // 12 is still 0, in another cell of the same element: nothing.
            ev("drag", "t", Some(1), None),
            ev("drag", "t", Some(2), None),
            ev("drag", "t", Some(8), None),
            ev("dragend", "t", Some(8), None),
        ]
    );
}

#[test]
fn steps_are_measured_against_the_dragged_element_wherever_the_pointer_goes() {
    let mut h = host();
    place(
        &mut h,
        "<div id=t data-on=drag data-steps=\"4 2\">t</div><div id=o data-on=drag>o</div>",
    );
    let mut got = at(&mut h, PointerKind::Down, 100.0, 10.0);
    for (x, y) in [(100.0, 30.0), (-50.0, 10.0), (500.0, 10.0), (500.0, 300.0), (150.0, 200.0)] {
        got.extend(at(&mut h, PointerKind::Move, x, y));
    }
    got.extend(at(&mut h, PointerKind::Up, 150.0, 200.0));
    assert_eq!(
        got,
        [
            ev("dragstart", "t", Some(2), Some(1)),
            // Over another element, below the box: clamped down.
            ev("drag", "o", Some(2), Some(2)),
            // Left of the surface, and right of it: clamped across.
            ev("drag", "", Some(0), Some(1)),
            ev("drag", "", Some(4), Some(1)),
            ev("drag", "", Some(4), Some(2)),
            ev("drag", "", Some(3), Some(2)),
            ev("dragend", "", Some(3), Some(2)),
        ]
    );
}

#[test]
fn a_touch_starts_at_the_step_where_it_began() {
    let mut h = host();
    // 10 pixels a step.
    place(
        &mut h,
        "<div id=t data-on=drag data-steps=20 style=\"touch-action:none\">t</div>",
    );
    let none = Mods::default();
    let mut got = drags(&h.touch("x", TouchPhase::Down, 14.0, 10.0, none).effects);
    for x in [75.0, 76.0, 300.0] {
        got.extend(drags(&h.touch("x", TouchPhase::Move, x, 10.0, none).effects));
    }
    got.extend(drags(&h.touch("x", TouchPhase::Move, 300.0, 60.0, none).effects));
    got.extend(drags(&h.touch("x", TouchPhase::Up, 300.0, 60.0, none).effects));
    assert_eq!(
        got,
        [
            ev("dragstart", "t", Some(1), None),
            ev("drag", "t", Some(8), None),
            // 300 is the surface's right edge: the window's, then past it.
            ev("drag", "", Some(20), None),
            ev("drag", "", Some(20), None),
            ev("dragend", "", Some(20), None),
        ]
    );
}

#[test]
fn an_element_that_leaves_keeps_its_last_steps_and_counts_are_read_at_the_start() {
    let mut h = host();
    place(&mut h, "<div id=t data-on=drag data-steps=4>t</div>");
    let mut got = at(&mut h, PointerKind::Down, 100.0, 10.0);
    got.extend(at(&mut h, PointerKind::Move, 150.0, 10.0));
    h.handle(&cmd(
        &[("a", "delta"), ("s", "x"), ("op", "remove"), ("t", "t"), ("q", "2")],
        "",
    ));
    got.extend(at(&mut h, PointerKind::Move, 50.0, 10.0));
    got.extend(at(&mut h, PointerKind::Move, 10.0, 10.0));
    got.extend(at(&mut h, PointerKind::Up, 10.0, 10.0));
    assert_eq!(
        got,
        [
            ev("dragstart", "t", Some(2), None),
            ev("drag", "t", Some(3), None),
            ev("drag", "", Some(3), None),
            ev("drag", "", Some(3), None),
            ev("dragend", "", Some(3), None),
        ]
    );

    // A drag that ends early (hide) carries the steps as they last were.
    place(&mut h, "<div id=t data-on=drag data-steps=4>t</div>");
    at(&mut h, PointerKind::Down, 100.0, 10.0);
    at(&mut h, PointerKind::Move, 150.0, 10.0);
    let hidden = drags(&h.handle(&cmd(&[("a", "hide"), ("s", "x"), ("q", "2")], "")));
    assert_eq!(hidden, [ev("dragend", "", Some(3), None)]);

    // Counts changed during a drag change nothing until the next one.
    place(&mut h, "<div id=t data-on=drag data-steps=4>t</div>");
    at(&mut h, PointerKind::Down, 100.0, 10.0);
    h.handle(&cmd(
        &[
            ("a", "delta"),
            ("s", "x"),
            ("op", "attr"),
            ("t", "t"),
            ("k", "data-steps"),
            ("q", "2"),
        ],
        "8",
    ));
    assert_eq!(
        at(&mut h, PointerKind::Move, 150.0, 10.0),
        [ev("drag", "t", Some(3), None)]
    );
    at(&mut h, PointerKind::Up, 150.0, 10.0);
    assert_eq!(
        at(&mut h, PointerKind::Down, 150.0, 10.0),
        [ev("dragstart", "t", Some(6), None)]
    );
}

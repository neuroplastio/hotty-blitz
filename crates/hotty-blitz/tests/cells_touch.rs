//! A finger in the cells (SPEC §9.6): the areas a program names with
//! `a=touch`, which screen keeps them, and what a touch in one reports of
//! the mouse. The shared vectors cover the common path; these the corners.

use hotty_blitz::{
    CellsTouch, Config, Effect, Host, Metrics, Mods, MouseReport, PointerKind, TouchPhase,
};
use hotty_wire::{Command, Event, Scanner};
use serde_json::{Value, json};

/// Cells 10×20 device pixels, at `scale` device pixels per CSS pixel.
fn host_at(scale: f32) -> Host {
    Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale,
        },
        ..Config::default()
    })
}

fn host() -> Host {
    host_at(1.0)
}

/// Sends `a=touch` with `body` (none for null) and returns the reply's
/// code if it was refused.
fn send(h: &mut Host, body: &Value) -> Option<String> {
    let payload = if body.is_null() {
        Vec::new()
    } else {
        rmp_serde::to_vec_named(body).unwrap()
    };
    let effects = h.handle(&Command::new(
        [("a", "touch"), ("n", "1")].into_iter().collect(),
        payload,
    ));
    let mut code = None;
    for (a, body) in messages(&effects) {
        if a == "err" {
            code = body["code"].as_str().map(str::to_string);
        }
    }
    code
}

fn areas(h: &mut Host, areas: Value) {
    assert_eq!(send(h, &json!({ "areas": areas })), None);
}

/// Each message among effects, as its `a` and its body (or its `e` for an
/// event).
fn messages(effects: &[Effect]) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    let mut s = Scanner::new();
    for e in effects {
        if let Effect::Reply(b) = e {
            s.feed(b, &mut |ev| {
                if let Event::Command(c) = ev {
                    let a = c.get("a").unwrap_or("").to_string();
                    let body = if a == "ev" {
                        json!(c.get("e"))
                    } else if c.payload.is_empty() {
                        Value::Null
                    } else {
                        rmp_serde::from_slice(&c.payload).unwrap()
                    };
                    out.push((a, body));
                }
            });
        }
    }
    out
}

/// The centre of the screen's cell (`c`, `r`), in device pixels.
fn cell(c: f32, r: f32) -> (f32, f32) {
    ((c + 0.5) * 10.0, (r + 0.5) * 20.0)
}

/// The reports as `kind c r`.
fn shown(reports: &[MouseReport]) -> Vec<String> {
    reports
        .iter()
        .map(|m| {
            let kind = match m.kind {
                PointerKind::Down => "down",
                PointerKind::Up => "up",
                _ => "move",
            };
            format!("{kind} {} {}", m.c, m.r)
        })
        .collect()
}

/// One phase of a touch at `at` in a program that reads the mouse, on the
/// screen: whose it is, and what it reports.
fn finger(
    h: &mut Host,
    phase: TouchPhase,
    at: (f32, f32),
    mods: Mods,
) -> (CellsTouch, Vec<String>) {
    let out = h.cells_touch(phase, at.0, at.1, mods, true, true);
    (out.touch, shown(&out.mouse))
}

fn none() -> Mods {
    Mods::default()
}

#[test]
fn a_body_that_is_no_map_of_areas_is_refused_and_extra_keys_are_not() {
    let mut h = host();
    let area = json!({"c": 0, "r": 0, "w": 1, "h": 1, "drag": "x"});
    for bad in [
        json!({"areas": [{"c": 0, "r": 0, "w": 1, "h": 1}]}),
        json!({"areas": [{"c": 0, "r": 0, "w": 1, "h": 0, "drag": "x"}]}),
        json!({"areas": [{"c": -1, "r": 0, "w": 1, "h": 1, "drag": "x"}]}),
        json!({"areas": [{"c": "0", "r": 0, "w": 1, "h": 1, "drag": "x"}]}),
        json!({"areas": [{"c": 0, "r": 0, "w": 1, "h": 1, "drag": ""}]}),
        json!({"areas": [{"c": 0, "r": 0, "w": 1, "h": 1, "drag": "X"}]}),
        json!({"areas": ["x"]}),
        json!({"areas": null}),
        json!({"area": [area]}),
        json!({"areas": vec![area.clone(); 257]}),
    ] {
        assert_eq!(send(&mut h, &bad).as_deref(), Some("EINVAL"), "{bad}");
    }
    // A key the body does not name is ignored, in the body and in an area;
    // a cell far past the screen is no error.
    for good in [
        json!({"areas": [area], "v": 2}),
        json!({"areas": [{"c": 9007199254740991_u64, "r": 0, "w": 1, "h": 1, "drag": "xy", "id": "s"}]}),
        json!({"areas": vec![area.clone(); 256]}),
    ] {
        assert_eq!(send(&mut h, &good), None, "{good}");
    }
}

#[test]
fn a_refused_body_leaves_the_areas_there_were() {
    let mut h = host();
    areas(
        &mut h,
        json!([{"c": 0, "r": 0, "w": 10, "h": 1, "drag": "x"}]),
    );
    assert_eq!(
        send(&mut h, &json!({"areas": [{}]})).as_deref(),
        Some("EINVAL")
    );
    finger(&mut h, TouchPhase::Down, cell(1.0, 0.0), none());
    let (touch, mouse) = finger(&mut h, TouchPhase::Move, cell(5.0, 0.0), none());
    assert_eq!((touch, mouse.len()), (CellsTouch::Mouse, 2));
}

#[test]
fn moves_within_the_slop_wait_and_a_tap_reports_where_it_lifted() {
    let mut h = host();
    areas(
        &mut h,
        json!([{"c": 0, "r": 0, "w": 10, "h": 2, "drag": "x"}]),
    );
    assert_eq!(
        finger(&mut h, TouchPhase::Down, (5.0, 15.0), none()),
        (CellsTouch::Undecided, vec![])
    );
    // 7 px: not past the 8 CSS px slop, but on the next cell.
    assert_eq!(
        finger(&mut h, TouchPhase::Move, (12.0, 15.0), none()),
        (CellsTouch::Undecided, vec![])
    );
    let (touch, mouse) = finger(&mut h, TouchPhase::Up, (12.0, 15.0), none());
    assert_eq!(touch, CellsTouch::Mouse);
    assert_eq!(mouse, ["down 1 0", "up 1 0"]);
}

#[test]
fn the_slop_is_in_css_pixels() {
    let mut h = host_at(2.0);
    areas(
        &mut h,
        json!([{"c": 0, "r": 0, "w": 10, "h": 2, "drag": "x"}]),
    );
    finger(&mut h, TouchPhase::Down, (5.0, 15.0), none());
    // 14 device px are 7 CSS px: still a tap.
    assert_eq!(
        finger(&mut h, TouchPhase::Move, (19.0, 15.0), none()).0,
        CellsTouch::Undecided
    );
    let (touch, mouse) = finger(&mut h, TouchPhase::Move, (21.0, 15.0), none());
    assert_eq!(touch, CellsTouch::Mouse);
    assert_eq!(mouse, ["down 0 0", "move 2 0"]);
}

#[test]
fn a_tie_pans_and_a_pan_stays_one() {
    // A tie is a pan whichever way the area drags, as on a surface whose
    // element pans neither way (SPEC §9.1).
    for drag in ["x", "y", "xy"] {
        let mut h = host();
        areas(
            &mut h,
            json!([{"c": 0, "r": 0, "w": 20, "h": 10, "drag": drag}]),
        );
        finger(&mut h, TouchPhase::Down, (50.0, 50.0), none());
        assert_eq!(
            finger(&mut h, TouchPhase::Move, (70.0, 70.0), none()),
            (CellsTouch::Terminal, vec![]),
            "{drag}"
        );
        // Back along the way the area drags: still the terminal's.
        assert_eq!(
            finger(&mut h, TouchPhase::Move, (150.0, 70.0), none()),
            (CellsTouch::Terminal, vec![])
        );
        assert_eq!(
            finger(&mut h, TouchPhase::Up, (150.0, 70.0), none()),
            (CellsTouch::Terminal, vec![])
        );
    }
    // Off the diagonal by a pixel, an `xy` area drags.
    let mut h = host();
    areas(
        &mut h,
        json!([{"c": 0, "r": 0, "w": 20, "h": 10, "drag": "xy"}]),
    );
    finger(&mut h, TouchPhase::Down, (50.0, 50.0), none());
    assert_eq!(
        finger(&mut h, TouchPhase::Move, (70.0, 71.0), none()).0,
        CellsTouch::Mouse
    );
}

#[test]
fn a_drag_reports_every_move_and_the_keys_held() {
    let mut h = host();
    areas(
        &mut h,
        json!([{"c": 0, "r": 0, "w": 10, "h": 1, "drag": "x"}]),
    );
    let shift = Mods {
        shift: true,
        ..none()
    };
    let ctrl = Mods {
        ctrl: true,
        ..none()
    };
    h.cells_touch(TouchPhase::Down, 5.0, 10.0, shift, true, true);
    // The press has the keys held when the touch began, the move those
    // held now. A move within the cell it began on follows no press.
    let out = h.cells_touch(TouchPhase::Move, 9.0 + 10.0, 10.0, ctrl, true, true);
    assert_eq!(shown(&out.mouse), ["down 0 0", "move 1 0"]);
    assert_eq!((out.mouse[0].mods, out.mouse[1].mods), (shift, ctrl));
    assert_eq!((out.mouse[0].x, out.mouse[0].y), (5.0, 10.0));
    // Off the screen, above it and left of it: the cell is negative.
    let out = h.cells_touch(TouchPhase::Move, -15.0, -25.0, ctrl, true, true);
    assert_eq!(shown(&out.mouse), ["move -2 -2"]);
    let out = h.cells_touch(TouchPhase::Move, -15.0, -25.0, ctrl, true, true);
    assert_eq!(shown(&out.mouse), ["move -2 -2"]);
    let out = h.cells_touch(TouchPhase::Up, 455.0, 10.0, none(), true, true);
    assert_eq!(shown(&out.mouse), ["up 45 0"]);
    assert_eq!(out.mouse[0].mods, none());
    // After the lift, a move is no one's.
    assert_eq!(
        finger(&mut h, TouchPhase::Move, (5.0, 10.0), none()),
        (CellsTouch::Terminal, vec![])
    );
}

#[test]
fn a_press_in_the_area_does_not_follow_a_move_within_its_cell() {
    let mut h = host();
    areas(
        &mut h,
        json!([{"c": 0, "r": 0, "w": 10, "h": 1, "drag": "y"}]),
    );
    finger(&mut h, TouchPhase::Down, (1.0, 1.0), none());
    // 9 px down: past the slop, still in cell (0, 0).
    let (touch, mouse) = finger(&mut h, TouchPhase::Move, (1.0, 10.0), none());
    assert_eq!(touch, CellsTouch::Mouse);
    assert_eq!(mouse, ["down 0 0"]);
}

#[test]
fn a_second_finger_releases_where_the_finger_last_was() {
    let mut h = host();
    areas(
        &mut h,
        json!([{"c": 0, "r": 0, "w": 10, "h": 1, "drag": "x"}]),
    );
    finger(&mut h, TouchPhase::Down, cell(1.0, 0.0), none());
    finger(&mut h, TouchPhase::Move, cell(6.0, 0.0), none());
    // Where the platform says it is at the cancel does not matter.
    let (touch, mouse) = finger(&mut h, TouchPhase::Cancel, cell(9.0, 3.0), none());
    assert_eq!(
        (touch, mouse),
        (CellsTouch::Terminal, vec!["up 6 0".to_string()])
    );
    // The rest of the gesture is the terminal's.
    assert_eq!(
        finger(&mut h, TouchPhase::Move, cell(8.0, 0.0), none()),
        (CellsTouch::Terminal, vec![])
    );
    assert_eq!(
        finger(&mut h, TouchPhase::Up, cell(8.0, 0.0), none()),
        (CellsTouch::Terminal, vec![])
    );
}

#[test]
fn a_second_finger_or_a_long_press_before_a_drag_makes_no_drag_nor_tap() {
    for phase in [TouchPhase::Cancel, TouchPhase::LongPress] {
        let mut h = host();
        areas(
            &mut h,
            json!([{"c": 0, "r": 0, "w": 10, "h": 1, "drag": "x"}]),
        );
        finger(&mut h, TouchPhase::Down, cell(1.0, 0.0), none());
        assert_eq!(
            finger(&mut h, phase, cell(1.0, 0.0), none()),
            (CellsTouch::Terminal, vec![])
        );
        assert_eq!(
            finger(&mut h, TouchPhase::Move, cell(6.0, 0.0), none()),
            (CellsTouch::Terminal, vec![])
        );
        assert_eq!(
            finger(&mut h, TouchPhase::Up, cell(6.0, 0.0), none()),
            (CellsTouch::Terminal, vec![])
        );
    }
    // A long press during a drag changes nothing.
    let mut h = host();
    areas(
        &mut h,
        json!([{"c": 0, "r": 0, "w": 10, "h": 1, "drag": "x"}]),
    );
    finger(&mut h, TouchPhase::Down, cell(1.0, 0.0), none());
    finger(&mut h, TouchPhase::Move, cell(6.0, 0.0), none());
    assert_eq!(
        finger(&mut h, TouchPhase::LongPress, cell(6.0, 0.0), none()),
        (CellsTouch::Mouse, vec![])
    );
    assert_eq!(
        finger(&mut h, TouchPhase::Up, cell(7.0, 0.0), none()),
        (CellsTouch::Mouse, vec!["up 7 0".to_string()])
    );
}

#[test]
fn new_areas_end_no_drag_and_a_touch_that_never_lifted_is_released() {
    let mut h = host();
    areas(
        &mut h,
        json!([{"c": 0, "r": 0, "w": 10, "h": 1, "drag": "x"}]),
    );
    finger(&mut h, TouchPhase::Down, cell(1.0, 0.0), none());
    finger(&mut h, TouchPhase::Move, cell(6.0, 0.0), none());
    assert_eq!(send(&mut h, &Value::Null), None);
    assert_eq!(
        finger(&mut h, TouchPhase::Move, cell(7.0, 0.0), none()),
        (CellsTouch::Mouse, vec!["move 7 0".to_string()])
    );
    // The next touch begins while the drag is under way, its lift lost:
    // the drag is released where its finger last was. The new touch is in
    // no area, as there are none now.
    let (touch, mouse) = finger(&mut h, TouchPhase::Down, cell(2.0, 0.0), none());
    assert_eq!(
        (touch, mouse),
        (CellsTouch::Terminal, vec!["up 7 0".to_string()])
    );
    // One that never decided releases nothing.
    areas(
        &mut h,
        json!([{"c": 0, "r": 0, "w": 10, "h": 1, "drag": "x"}]),
    );
    finger(&mut h, TouchPhase::Down, cell(3.0, 0.0), none());
    let (touch, mouse) = finger(&mut h, TouchPhase::Down, cell(2.0, 0.0), none());
    assert_eq!((touch, mouse), (CellsTouch::Undecided, vec![]));
}

#[test]
fn an_area_applies_only_to_a_program_that_reads_the_mouse_on_the_screen() {
    let mut h = host();
    areas(
        &mut h,
        json!([{"c": 0, "r": 0, "w": 10, "h": 1, "drag": "x"}]),
    );
    let (x, y) = cell(1.0, 0.0);
    for (reports, at_screen) in [(false, true), (true, false), (false, false)] {
        let out = h.cells_touch(TouchPhase::Down, x, y, none(), reports, at_screen);
        assert_eq!(out.touch, CellsTouch::Terminal);
        let out = h.cells_touch(TouchPhase::Up, x, y, none(), true, true);
        assert_eq!(
            (out.touch, out.mouse.len()),
            (CellsTouch::Terminal, 0),
            "a tap"
        );
    }
    // Alt held is the terminal's, and so is a cell outside every area.
    let alt = Mods {
        alt: true,
        ..none()
    };
    assert_eq!(
        h.cells_touch(TouchPhase::Down, x, y, alt, true, true).touch,
        CellsTouch::Terminal
    );
    let (x, y) = cell(10.0, 0.0);
    assert_eq!(
        h.cells_touch(TouchPhase::Down, x, y, none(), true, true)
            .touch,
        CellsTouch::Terminal
    );
}

#[test]
fn each_screen_keeps_its_areas_and_a_full_reset_drops_both() {
    let mut h = host();
    h.set_screen(false);
    areas(
        &mut h,
        json!([{"c": 0, "r": 0, "w": 10, "h": 1, "drag": "x"}]),
    );
    h.set_screen(true);
    areas(
        &mut h,
        json!([{"c": 0, "r": 1, "w": 10, "h": 1, "drag": "x"}]),
    );
    let starts = |h: &mut Host, r: f32| {
        let (x, y) = cell(1.0, r);
        let touch = h
            .cells_touch(TouchPhase::Down, x, y, none(), true, true)
            .touch;
        h.cells_touch(TouchPhase::Cancel, x, y, none(), true, true);
        touch
    };
    assert_eq!(
        (starts(&mut h, 0.0), starts(&mut h, 1.0)),
        (CellsTouch::Terminal, CellsTouch::Undecided)
    );
    // `a=del` deletes surfaces, not the cells' areas.
    h.handle(&Command::new(
        [("a", "del"), ("q", "2")].into_iter().collect(),
        Vec::new(),
    ));
    assert_eq!(starts(&mut h, 1.0), CellsTouch::Undecided);
    h.set_screen(false);
    assert_eq!(
        (starts(&mut h, 0.0), starts(&mut h, 1.0)),
        (CellsTouch::Undecided, CellsTouch::Terminal)
    );
    // Back on the alternate screen: the areas sent there went.
    h.set_screen(true);
    assert_eq!(starts(&mut h, 1.0), CellsTouch::Terminal);
    areas(
        &mut h,
        json!([{"c": 0, "r": 1, "w": 10, "h": 1, "drag": "x"}]),
    );
    h.reset();
    assert_eq!(starts(&mut h, 1.0), CellsTouch::Terminal);
    h.set_screen(false);
    assert_eq!(starts(&mut h, 0.0), CellsTouch::Terminal);
}

#[test]
fn the_press_gives_the_keyboard_back() {
    let mut h = host();
    let cmd = |pairs: &[(&str, &str)], payload: &str| {
        Command::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
    };
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x"), ("q", "2")],
        "<input id=i>",
    ));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "20"),
            ("r", "2"),
            ("q", "2"),
        ],
        "",
    ));
    h.handle(&cmd(
        &[("a", "focus"), ("s", "x"), ("t", "i"), ("q", "2")],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    assert_eq!(h.focused_surface(), Some("x"));
    areas(
        &mut h,
        json!([{"c": 0, "r": 5, "w": 10, "h": 1, "drag": "x"}]),
    );
    let (x, y) = cell(1.0, 5.0);
    // A touch that pans takes nothing.
    h.cells_touch(TouchPhase::Down, x, y, none(), true, true);
    let out = h.cells_touch(TouchPhase::Move, x, y + 60.0, none(), true, true);
    assert_eq!((out.touch, out.effects.len()), (CellsTouch::Terminal, 0));
    h.cells_touch(TouchPhase::Up, x, y + 60.0, none(), true, true);
    assert_eq!(h.focused_surface(), Some("x"));
    // A tap is a click on the cells.
    h.cells_touch(TouchPhase::Down, x, y, none(), true, true);
    let out = h.cells_touch(TouchPhase::Up, x, y, none(), true, true);
    assert_eq!(out.touch, CellsTouch::Mouse);
    let events: Vec<_> = messages(&out.effects).into_iter().map(|(_, e)| e).collect();
    assert_eq!(events, [json!("blur")]);
    assert_eq!(h.focused_surface(), None);
}

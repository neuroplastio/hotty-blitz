//! CSS animations and transitions play over time, and stop.

mod common;

use common::delta_cmd;
use hotty::Message;
use hotty_blitz::{Config, Damage, Host, Metrics};
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

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

fn render(h: &mut Host) -> Vec<(String, u32, u32, Damage)> {
    let mut frames = Vec::new();
    h.render_dirty(&mut |name, f, damage| {
        frames.push((name.to_string(), f.width, f.height, damage.clone()))
    });
    frames
}

/// The pixel at `(x, y)` of the surface's last frame, as RGB.
fn pixel(h: &Host, s: &str, x: u32, y: u32) -> [u8; 3] {
    let f = h.frame(s).unwrap();
    let i = ((y * f.width + x) * 4) as usize;
    [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
}

/// An 80x10 surface (800x200 px, large enough that a small change repaints
/// only part of it) whose document is `html`.
fn doc(h: &mut Host, html: &str) {
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], html));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "80"), ("r", "10")],
        "",
    ));
    render(h);
}

const RED: [u8; 3] = [255, 0, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const GREEN: [u8; 3] = [0, 128, 0];

/// The colour `p` of the way from `from` to `to` rounds to.
fn at(from: [u8; 3], to: [u8; 3], p: f64) -> [u8; 3] {
    [0, 1, 2]
        .map(|c| (f64::from(from[c]) + (f64::from(to[c]) - f64::from(from[c])) * p).round() as u8)
}

/// The colour the transition or animation shows somewhere between `lo` and
/// `hi` of the way from `from` to `to`: each channel between what the ends
/// round to, a rounding step either side.
fn between(got: [u8; 3], from: [u8; 3], to: [u8; 3], lo: f64, hi: f64) {
    let (a, b) = (at(from, to, lo), at(from, to, hi));
    for c in 0..3 {
        let (x, y) = (a[c].min(b[c]).saturating_sub(1), a[c].max(b[c]) + 1);
        assert!(
            got[c] >= x && got[c] <= y,
            "channel {c} is {}, between {x} and {y} (at {lo}..={hi} of the way)",
            got[c]
        );
    }
}

/// A `div` that transitions its background from red to blue, `8s` linear,
/// at the top left of the surface.
const BOX: &str = "position:absolute;left:0;top:0;width:50px;height:50px;";

#[test]
fn a_transition_animates_over_its_duration_and_stops() {
    let mut h = host();
    doc(
        &mut h,
        &format!(
            "<body style='margin:0'><div id=a style='{BOX}background:rgb(255,0,0);transition:background-color 8s linear'></div></body>"
        ),
    );
    assert!(h.next_frame().is_none(), "nothing is animating yet");
    assert_eq!(pixel(&h, "x", 10, 10), RED);

    // The style changes: the transition starts at the render that shows it.
    h.handle(&delta_cmd(
        &[("s", "x"), ("n", "1")],
        "attr",
        Some("a"),
        Some("style"),
        &format!("{BOX}background:rgb(0,0,255);transition:background-color 8s linear"),
    ));
    let before = Instant::now();
    render(&mut h);
    let after = Instant::now();
    assert_eq!(
        pixel(&h, "x", 10, 10),
        RED,
        "a transition starts at its from value"
    );
    assert!(h.next_frame().is_some(), "the transition wants frames");

    // A quarter of the way in, and three quarters, whatever the render that
    // started it took: the transition began between `before` and `after`.
    for (secs, p) in [(2.0, 0.25), (6.0, 0.75)] {
        h.animate(after + Duration::from_secs_f64(secs));
        render(&mut h);
        between(
            pixel(&h, "x", 10, 10),
            RED,
            BLUE,
            p,
            p + (after - before).as_secs_f64() / 8.0,
        );
    }

    // Its box, and only its box, is repainted as it goes.
    h.animate(after + Duration::from_secs_f64(6.5));
    match render(&mut h).remove(0).3 {
        Damage::Rects(rs) => assert!(
            rs.iter().all(|r| r.x + r.w <= 80 && r.y + r.h <= 80),
            "{rs:?}"
        ),
        d => panic!("a transition repaints the div's box, not the frame: {d:?}"),
    }

    // Past its end the transition is gone: the style stands, and the host
    // asks for no frame any more.
    h.animate(after + Duration::from_secs_f64(9.0));
    render(&mut h);
    assert_eq!(pixel(&h, "x", 10, 10), BLUE);
    assert!(h.next_frame().is_none(), "the transition has ended");
}

#[test]
fn a_keyframes_animation_plays_its_iterations_and_stops() {
    let mut h = host();
    let before = Instant::now();
    doc(
        &mut h,
        "<style>@keyframes go{from{background:rgb(255,0,0)}to{background:rgb(0,0,255)}}</style>\
<body style='margin:0'>\
<div id=a style='position:absolute;left:0;top:0;width:50px;height:50px;\
background:rgb(0,128,0);animation:go 4s linear 2'></div></body>",
    );
    let after = Instant::now();
    assert_eq!(
        pixel(&h, "x", 10, 10),
        RED,
        "the animation starts at its from keyframe"
    );
    assert!(h.next_frame().is_some(), "the animation wants frames");

    // Halfway through the first iteration, and a quarter into the second:
    // the same clock runs across both.
    let dt = (after - before).as_secs_f64() / 4.0;
    for (secs, p) in [(2.0, 0.5), (5.0, 0.25)] {
        h.animate(after + Duration::from_secs_f64(secs));
        render(&mut h);
        between(pixel(&h, "x", 10, 10), RED, BLUE, p, p + dt);
    }

    // Two iterations of 4s, no fill: the element is back at its style.
    h.animate(after + Duration::from_secs_f64(8.5));
    render(&mut h);
    assert_eq!(pixel(&h, "x", 10, 10), GREEN);
    assert!(h.next_frame().is_none(), "the animation has ended");
}

/// A green box animated with `animation`, its background from red to blue.
fn animated(h: &mut Host, animation: &str) {
    doc(
        h,
        &format!(
            "<style>@keyframes go{{from{{background:rgb(255,0,0)}}to{{background:rgb(0,0,255)}}}}</style>\
<body style='margin:0'><div id=a style='{BOX}background:rgb(0,128,0);animation:{animation}'></div></body>"
        ),
    );
}

/// The frames the host is asked for over the second after the next one,
/// rendering each when it is due, and the frames that drew something.
fn frames_in_a_second(h: &mut Host) -> (usize, usize) {
    let start = h.next_frame().expect("the animation wants frames");
    let (mut asked, mut drawn) = (0, 0);
    let mut t = start;
    while t < start + Duration::from_secs(1) {
        h.animate(t);
        asked += 1;
        drawn += render(h).len();
        t = h.next_frame().expect("the animation wants frames");
    }
    (asked, drawn)
}

#[test]
fn a_steps_animation_asks_for_a_frame_at_each_step() {
    // Ten steps a second, for ten seconds.
    let mut h = host();
    animated(&mut h, "go 10s steps(100)");
    let (asked, drawn) = frames_in_a_second(&mut h);
    assert!((9..=11).contains(&asked), "{asked} frames asked for");
    assert!((9..=11).contains(&drawn), "{drawn} frames drawn");
}

#[test]
fn a_linear_animation_asks_for_every_frame() {
    let mut h = host();
    animated(&mut h, "go 10s linear");
    let (asked, drawn) = frames_in_a_second(&mut h);
    assert!((59..=61).contains(&asked), "{asked} frames asked for");
    assert!((59..=61).contains(&drawn), "{drawn} frames drawn");
}

#[test]
fn an_animation_in_its_delay_asks_for_nothing_until_it_starts() {
    let mut h = host();
    let before = Instant::now();
    animated(&mut h, "go 10s linear 2s");
    let after = Instant::now();
    let due = h.next_frame().expect("the animation will start");
    assert!(
        due >= before + Duration::from_secs(2) && due <= after + Duration::from_millis(2010),
        "the first frame is due when the delay ends, {:?} after the document",
        due - before
    );
    assert_eq!(
        pixel(&h, "x", 10, 10),
        GREEN,
        "before it starts, its own style"
    );
    h.animate(due);
    render(&mut h);
    let p = pixel(&h, "x", 10, 10);
    assert!(
        p[0] >= 250 && p[2] <= 5,
        "it starts at its from keyframe: {p:?}"
    );
    let (asked, _) = frames_in_a_second(&mut h);
    assert!((59..=61).contains(&asked), "{asked} frames asked for");
}

#[test]
fn a_paused_animation_asks_for_nothing() {
    let mut h = host();
    animated(&mut h, "go 10s linear paused");
    assert!(
        h.next_frame().is_none(),
        "a paused animation wants no frames"
    );
}

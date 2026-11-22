//! Motion is paced at the display's refresh rate, as the terminal gives it
//! (`Host::set_refresh_rate`): CSS animations and scrollbars fading ask for
//! a frame each frame of the display, at 60 Hz until it is given.

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics, Mods};
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

fn render(h: &mut Host) -> usize {
    let mut drawn = 0;
    h.render_dirty(&mut |_, _, _| drawn += 1);
    drawn
}

const ANIMATED: &str = "<style>@keyframes go{from{background:rgb(255,0,0)}to{background:rgb(0,0,255)}}</style>\
<body style='margin:0'><div style='position:absolute;left:0;top:0;width:50px;height:50px;\
animation:go 1000s linear'></div></body>";

/// Surface `x`, 80×10, whose document is `html`, with `extra` keys on its
/// `a=doc`, placed and drawn.
fn doc(h: &mut Host, extra: &[(&str, &str)], html: &str) {
    let mut pairs = vec![("a", "doc"), ("s", "x")];
    pairs.extend_from_slice(extra);
    h.handle(&cmd(&pairs, html));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "80"), ("r", "10")],
        "",
    ));
    render(h);
}

/// The frames a linear animation asks for over a second, rendering each
/// when it is due. The second sampled is well ahead of the clock, so that
/// rendering time cannot overtake the test's own deadlines.
fn css_frames_in_a_second(h: &mut Host) -> usize {
    let future = h.next_frame().expect("the animation wants frames") + Duration::from_secs(30);
    h.animate(future);
    render(h);
    let start = h.next_frame().expect("the animation wants frames");
    let mut asked = 0;
    let mut t = start;
    while t < start + Duration::from_secs(1) {
        h.animate(t);
        asked += 1;
        assert_eq!(render(h), 1, "each frame asked for draws");
        t = h.next_frame().expect("the animation wants frames");
    }
    asked
}

fn around(got: usize, want: usize) {
    assert!(
        got.abs_diff(want) <= 1,
        "{got} frames asked for, not {want}"
    );
}

#[test]
fn a_css_animation_asks_for_frames_at_the_rate_given() {
    let mut h = host();
    doc(&mut h, &[], ANIMATED);
    around(css_frames_in_a_second(&mut h), 60);
    h.set_refresh_rate(120_000);
    around(css_frames_in_a_second(&mut h), 120);
    h.set_refresh_rate(30_000);
    around(css_frames_in_a_second(&mut h), 30);
    // Unknown again: 60 Hz.
    h.set_refresh_rate(0);
    around(css_frames_in_a_second(&mut h), 60);
}

/// When a wheel turned the root, and the instants frames are asked for at
/// from then until nothing is due, each frame advanced to when it is due.
/// Not rendered: a render paces the next frame from the real clock once it
/// has caught up with these, and a slow build's would.
fn fade_frames(h: &mut Host) -> (Instant, Vec<Instant>) {
    fade_fired(h, |_, t| t)
}

/// As [`fade_frames`], each frame advanced to the instant `fire` gives for
/// it, from when it is due.
fn fade_fired(
    h: &mut Host,
    mut fire: impl FnMut(&mut Host, Instant) -> Instant,
) -> (Instant, Vec<Instant>) {
    let rows: String = (0..20).map(|i| format!("<div>{i}</div>")).collect();
    let html = format!("<style>body{{margin:0}}div{{height:20px}}</style>{rows}");
    doc(h, &[("scroll", "1")], &html);
    let turned = Instant::now();
    assert!(h.wheel("x", 15.0, 10.0, 0.0, 30.0, Mods::default()).taken);
    h.end_gesture();
    render(h);
    let mut due = Vec::new();
    while let Some(t) = h.next_frame() {
        assert!(t < turned + Duration::from_secs(2), "the fade ends");
        let at = fire(h, t);
        h.animate(at);
        due.push(t);
    }
    (turned, due)
}

/// The frames of a fade's 200 ms, which begins 500 ms after the wheel
/// turned (`turned`, a hair before): a frame that the real clock asks for
/// besides, such as the wheel's scroll coming to rest after a slow render,
/// is no frame of the fade.
fn the_fade((turned, frames): (Instant, Vec<Instant>)) -> Vec<Instant> {
    let mut fade = frames
        .into_iter()
        .skip_while(|t| *t < turned + Duration::from_millis(500));
    let Some(first) = fade.next() else {
        return Vec::new();
    };
    std::iter::once(first)
        .chain(fade.take_while(|t| *t <= first + Duration::from_millis(200)))
        .collect()
}

/// The gaps between the frames of a fade.
fn fade_gaps(frames: &[Instant]) -> Vec<Duration> {
    frames.windows(2).map(|w| w[1] - w[0]).collect()
}

#[test]
fn a_scrollbar_fades_at_the_rate_given() {
    for (mhz, hz) in [(0, 60), (60_000, 60), (120_000, 120), (30_000, 30)] {
        let mut h = host();
        h.set_refresh_rate(mhz);
        let frames = the_fade(fade_frames(&mut h));
        let frame = Duration::from_nanos(1_000_000_000_000 / (hz * 1000));
        assert_eq!(h.frame_interval(), frame);
        let gaps = fade_gaps(&frames);
        assert!(!gaps.is_empty(), "the fade takes frames at {hz} Hz");
        assert!(
            gaps.iter().all(|g| *g == frame),
            "every frame of the fade a {hz} Hz frame after the last: {gaps:?}"
        );
        // A fade of 200 ms.
        around(frames.len(), 200 * hz as usize / 1000 + 1);
    }
}

#[test]
fn a_rate_is_sixty_until_given_and_kept_within_reason() {
    let mut h = host();
    let sixty = Duration::from_nanos(1_000_000_000 / 60);
    assert_eq!(h.frame_interval(), sixty);
    h.set_refresh_rate(144_000);
    assert_eq!(h.frame_interval(), Duration::from_nanos(6_944_444));
    // GTK's millihertz, as a monitor gives them.
    h.set_refresh_rate(59_951);
    assert_eq!(h.frame_interval(), Duration::from_nanos(16_680_288));
    h.set_refresh_rate(1_000_000);
    assert_eq!(
        h.frame_interval(),
        Duration::from_nanos(2_777_777),
        "at most 360 Hz"
    );
    h.set_refresh_rate(1_000);
    assert_eq!(
        h.frame_interval(),
        Duration::from_nanos(41_666_666),
        "at least 24 Hz"
    );
    h.set_refresh_rate(0);
    assert_eq!(h.frame_interval(), sixty, "unknown is 60 Hz");
}

#[test]
fn a_rate_holds_for_a_document_replaced_detached_or_made_after_a_reset() {
    let mut h = host();
    h.set_refresh_rate(120_000);
    doc(&mut h, &[], ANIMATED);
    around(css_frames_in_a_second(&mut h), 120);
    // Detached (SPEC §5.5), then the program's again: new documents both.
    doc(&mut h, &[("d", "1")], ANIMATED);
    doc(&mut h, &[], ANIMATED);
    around(css_frames_in_a_second(&mut h), 120);
    // The program's session ends; the next one's surfaces keep the rate.
    h.reset();
    doc(&mut h, &[], ANIMATED);
    around(css_frames_in_a_second(&mut h), 120);
}

#[test]
fn a_rate_changed_mid_fade_paces_the_rest_of_it() {
    let mut h = host();
    // 60 Hz for the fade's first four frames, then 120.
    let mut n = 0;
    let frames = the_fade(fade_fired(&mut h, |h, t| {
        n += 1;
        if n == 4 {
            h.set_refresh_rate(120_000);
        }
        t
    }));
    let gaps = fade_gaps(&frames);
    assert!(gaps.len() > 8, "{gaps:?}");
    let (sixty, one_twenty) = gaps.split_at(3);
    assert!(
        sixty.iter().all(|g| *g == Duration::from_nanos(16_666_666)),
        "{gaps:?}"
    );
    assert!(
        one_twenty
            .iter()
            .all(|g| *g == Duration::from_nanos(8_333_333)),
        "{gaps:?}"
    );
}

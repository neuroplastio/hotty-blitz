//! An infinite CSS animation whose every frame paints nothing (it plays
//! where the surface does not reach, or in a box that clips it away): once
//! a whole period of it has painted nothing, the surface asks for no more
//! frames, until something besides the animation changes it.

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};
use std::time::{Duration, Instant};

/// Frames run this far ahead of the clock, so that a resolve never takes
/// the clock's time instead (`Host::animate` moves time forward only).
const AHEAD: Duration = Duration::from_secs(3600);

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

fn host() -> Host {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    h.set_refresh_rate(60_000);
    h
}

fn render(h: &mut Host) -> usize {
    let mut drawn = 0;
    h.render_dirty(&mut |_, _, _| drawn += 1);
    drawn
}

const STYLE: &str = "<style>body{margin:0;background:#111}
@keyframes p{50%{opacity:.2}}
@keyframes m{50%{transform:translateY(-60px)}}
.dot{width:10px;height:10px;background:#6c6;animation:p 400ms linear infinite}
</style>";

/// Places `body` on a surface 8 columns by 2 rows (80 by 40 pixels) and
/// plays it frame by frame for `periods` periods of 400 ms: the frames
/// delivered, and whether it still asks for one after.
fn play(body: &str, periods: u32) -> (Host, Instant, usize) {
    let mut h = host();
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], &format!("{STYLE}{body}")));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "8"), ("r", "2")],
        "",
    ));
    render(&mut h);
    let mut t = Instant::now() + AHEAD;
    let mut delivered = 0;
    let frames = periods * 24;
    for _ in 0..frames {
        let Some(due) = h.next_frame() else { break };
        t = t.max(due) + Duration::from_micros(1);
        h.animate(t);
        delivered += render(&mut h);
    }
    (h, t, delivered)
}

#[test]
fn an_animation_outside_the_surface_stops_asking_for_frames() {
    let (h, _, delivered) = play(
        "<div style='position:absolute;top:200px' class=dot></div>",
        3,
    );
    assert_eq!(delivered, 0, "nothing it moves is on the surface");
    assert_eq!(h.next_frame(), None, "a period of nothing: no more frames");
}

#[test]
fn an_animation_its_box_clips_away_stops_asking_for_frames() {
    let (h, _, delivered) = play(
        "<div style='height:0;overflow:hidden'><div class=dot></div></div>",
        3,
    );
    // Its box is where damage goes, so frames go out until a period has
    // been kept and every phase of it is the same.
    assert!(delivered <= 2 * 24, "{delivered}");
    assert_eq!(h.next_frame(), None);
}

#[test]
fn a_seen_animation_keeps_playing() {
    let (h, _, delivered) = play("<div class=dot></div>", 3);
    assert!(delivered > 40, "it plays every frame: {delivered}");
    assert!(h.next_frame().is_some());
}

#[test]
fn an_animation_seen_for_part_of_its_period_keeps_playing() {
    // Below the surface, it comes up into it halfway through.
    let (h, _, delivered) = play(
        "<div style='position:absolute;top:70px;width:10px;height:10px;\
         background:#6c6;animation:m 400ms linear infinite'></div>",
        3,
    );
    assert!(delivered > 0, "it is seen while it is up");
    assert!(h.next_frame().is_some(), "and asks for frames still");
}

#[test]
fn a_change_brings_it_back_into_view() {
    let (mut h, mut t, _) = play(
        "<div id=d style='position:absolute;top:200px' class=dot></div>",
        3,
    );
    assert_eq!(h.next_frame(), None);
    let ops = rmp_serde::to_vec_named(&serde_json::json!({
        "ops": [{"op": "attr", "t": "d", "k": "style", "v": "position:absolute;top:0"}]
    }))
    .unwrap();
    h.handle(&Message::new(
        [("a", "delta"), ("s", "x")].into_iter().collect(),
        ops,
    ));
    let mut delivered = render(&mut h);
    for _ in 0..24 {
        let Some(due) = h.next_frame() else { break };
        t = t.max(due) + Duration::from_micros(1);
        h.animate(t);
        delivered += render(&mut h);
    }
    assert!(delivered > 10, "in view, it plays again: {delivered}");
}

#[test]
fn a_change_unclips_it() {
    let (mut h, mut t, _) = play(
        "<div id=c style='height:0;overflow:hidden'><div class=dot></div></div>",
        3,
    );
    assert_eq!(h.next_frame(), None);
    let ops = rmp_serde::to_vec_named(&serde_json::json!({
        "ops": [{"op": "attr", "t": "c", "k": "style", "v": "height:20px;overflow:hidden"}]
    }))
    .unwrap();
    h.handle(&Message::new(
        [("a", "delta"), ("s", "x")].into_iter().collect(),
        ops,
    ));
    let mut delivered = render(&mut h);
    for _ in 0..24 {
        let Some(due) = h.next_frame() else { break };
        t = t.max(due) + Duration::from_micros(1);
        h.animate(t);
        delivered += render(&mut h);
    }
    assert!(delivered > 10, "unclipped, it plays again: {delivered}");
}

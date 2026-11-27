//! Infinite CSS animations played from memory: once a loop has been seen
//! through its period, its frames are what a full paint at the same moment
//! paints. Time is driven ahead of the clock (`Host::animate` moves a
//! surface's time forward only), so every frame is at a known moment.

mod common;

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};
use std::time::{Duration, Instant};

/// How far ahead of the clock the frames are: further than any run takes,
/// so that a resolve never takes the clock's time instead.
const AHEAD: Duration = Duration::from_secs(3600);

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

fn host() -> Host {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 2.0,
        },
        ..Config::default()
    });
    h.set_refresh_rate(60_000);
    h
}

fn render(h: &mut Host) -> Vec<hotty_blitz::Damage> {
    let mut out = Vec::new();
    h.render_dirty(&mut |_, _, d| out.push(d.clone()));
    out
}

const STYLE: &str = "<style>
  body{margin:0;font:14px sans-serif;background:#1b1b1f;color:#ddd}
  .bar{display:flex;gap:8px;align-items:center;padding:6px 8px}
  @keyframes p{0%,100%{opacity:1}50%{opacity:.3}}
  @keyframes r{to{transform:rotate(360deg)}}
  @keyframes f{50%{opacity:.1}}
  @keyframes grow{50%{width:120px}}
  .dot{width:10px;height:10px;border-radius:50%;background:#6c6;animation:p 400ms ease-in-out infinite}
  .spin{display:inline-flex}.spin>svg{animation:r 400ms linear infinite}
  .t i{display:inline-block;width:5px;height:5px;border-radius:50%;background:currentColor;opacity:.35;margin-left:4px}
  .d1{animation:p 400ms linear infinite}.d2{animation:p 400ms linear infinite .1s}.d3{animation:p 400ms linear infinite .2s}
  .mini{position:relative;height:120px;margin:8px;border:1px solid #555}
  .tint{position:absolute;top:0;right:0;bottom:0;left:0;background:#48f;opacity:.35;animation:f 800ms ease-in-out infinite}
  .tint ~ *{position:relative}
  .grow{height:10px;width:20px;background:#c66;animation:grow 400ms linear infinite}
  .s2{animation:p 400ms linear infinite .3s}.s3{animation:p 400ms linear infinite .6s}
  .fast{margin:0 24px}.fast>svg{width:48px;height:48px;animation-duration:350ms}
  .orbit{width:8px;height:8px;margin:0 14px;background:#6c6;box-shadow:12px 0 0 #c66;animation:r 400ms linear infinite}
</style>";

const SPIN: &str = "<span class=spin><svg width=16 height=16 viewBox='0 0 16 16'><circle cx=8 cy=8 r=6 stroke='#6af' stroke-width=2 fill=none stroke-dasharray='24 14'/></svg></span>";

/// Plays `body` for five periods of `period` and returns how many frames
/// came from memory; every frame of the last of them is checked against a
/// full paint at the same moment, one period later.
fn plays_as_painted(body: &str, cols: &str, rows: &str, period: Duration) -> u64 {
    plays_as_painted_from(body, cols, rows, period, AHEAD)
}

/// As `plays_as_painted`, the first frame `ahead` past the document's
/// start.
fn plays_as_painted_from(
    body: &str,
    cols: &str,
    rows: &str,
    period: Duration,
    ahead: Duration,
) -> u64 {
    let mut h = host();
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], &format!("{STYLE}{body}")));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", cols), ("r", rows)],
        "",
    ));
    render(&mut h);
    let fi = h.frame_interval();
    let per = (period.as_secs_f64() / fi.as_secs_f64()).round() as u32;
    let mut t = Instant::now() + ahead;
    let mut step = |h: &mut Host| {
        t += fi;
        h.animate(t);
        render(h);
        h.frame("x").unwrap().rgba.clone()
    };
    // Seen: two periods, the first to settle.
    for _ in 0..2 * per {
        step(&mut h);
    }
    let before = h.loop_frames("x");
    // From memory for three periods, nothing resolved: the first paint
    // after must find the animation where the clock is, not iterations
    // behind.
    let kept: Vec<Vec<u8>> = (0..3 * per)
        .map(|_| step(&mut h))
        .skip(2 * per as usize)
        .collect();
    let from_memory = h.loop_frames("x") - before;
    // Each painted whole, one period on.
    let w = h.frame("x").unwrap().width as usize;
    for (k, kept) in kept.iter().enumerate() {
        h.repaint();
        let painted = step(&mut h);
        let worst = kept
            .iter()
            .zip(&painted)
            .enumerate()
            .map(|(i, (a, b))| ((*a as i32 - *b as i32).abs(), i))
            .max()
            .unwrap();
        assert!(
            worst.0 <= 3,
            "frame {k}: {} off at pixel ({}, {}) channel {}",
            worst.0,
            worst.1 / 4 % w,
            worst.1 / 4 / w,
            worst.1 % 4,
        );
    }
    from_memory
}

#[test]
fn a_pulsing_dot_plays_from_memory() {
    let n = plays_as_painted(
        "<div class=bar><span class=dot></span><span>agent working</span></div>",
        "30",
        "2",
        Duration::from_millis(400),
    );
    assert!(n >= 20, "{n} frames from memory");
}

#[test]
fn a_spinner_plays_from_memory() {
    let n = plays_as_painted(
        &format!("<div class=bar>{SPIN}<span>building</span></div>"),
        "30",
        "2",
        Duration::from_millis(400),
    );
    assert!(n >= 20, "{n} frames from memory");
}

#[test]
fn typing_dots_with_delays_play_from_memory() {
    let n = plays_as_painted(
        "<div class=bar><span class=t><i class=d1></i><i class=d2></i><i class=d3></i></span><span>typing</span></div>",
        "30",
        "2",
        Duration::from_millis(400),
    );
    assert!(n >= 20, "{n} frames from memory");
}

#[test]
fn delayed_loops_play_from_memory_once_all_have_started() {
    // From the document's start: the later dots start while the first is
    // watched, which stops that watch.
    let n = plays_as_painted_from(
        "<div class=bar><span class=t><i class=d1></i><i class=s2></i><i class=s3></i></span><span>typing</span></div>",
        "30",
        "2",
        Duration::from_millis(1200),
        Duration::ZERO,
    );
    assert!(n >= 60, "{n} frames from memory");
}

#[test]
fn a_large_fade_under_text_plays_as_a_blend() {
    // Too large to keep each frame: the two ends are blended, under the
    // text painted after it.
    let minis: String = (0..3)
        .map(|i| {
            format!("<div class=mini><div class=tint></div><b>pane {i}</b><p>output line</p></div>")
        })
        .collect();
    let n = plays_as_painted(&minis, "40", "20", Duration::from_millis(800));
    assert!(n >= 40, "{n} frames from memory");
}

#[test]
fn loops_side_by_side_play_each_its_own_period() {
    // A dot beside a spinner, nearer than the damage tracker merges: kept
    // as one, their common period (2.8 s) would not fit the budget.
    let n = plays_as_painted(
        &format!(
            "<div class=bar><span class=dot></span><span class=\"spin fast\">{}</span><span>working</span></div>",
            &SPIN[SPIN.find('>').unwrap() + 1..SPIN.rfind('<').unwrap()]
        ),
        "40",
        "8",
        Duration::from_millis(2800),
    );
    assert!(n >= 300, "{n} frames from memory");
}

#[test]
fn a_loop_whose_shadow_reaches_past_its_box_is_painted() {
    let n = plays_as_painted(
        "<div class=bar><span class=orbit></span><span>agent working</span></div>",
        "30",
        "2",
        Duration::from_millis(400),
    );
    assert_eq!(n, 0, "a shadow outside the kept region is not kept");
}

#[test]
fn an_animation_that_moves_layout_is_painted() {
    let n = plays_as_painted(
        "<div class=bar><div class=grow></div><span>pushed</span></div>",
        "30",
        "2",
        Duration::from_millis(400),
    );
    assert_eq!(n, 0, "a loop that moves others is not kept");
}

#[test]
fn a_change_beside_the_loop_paints_and_watches_again() {
    let mut h = host();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x")],
        &format!("{STYLE}<div class=bar><span class=dot></span><span id=s>one</span></div>"),
    ));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "30"), ("r", "2")],
        "",
    ));
    render(&mut h);
    let fi = h.frame_interval();
    let mut t = Instant::now() + AHEAD;
    for _ in 0..80 {
        t += fi;
        h.animate(t);
        render(&mut h);
    }
    assert!(h.loop_frames("x") > 0);
    let before = h.loop_frames("x");
    h.handle(&common::delta_cmd(
        &[("s", "x")],
        "text",
        Some("s"),
        None,
        "two",
    ));
    t += fi;
    h.animate(t);
    render(&mut h);
    assert_eq!(h.loop_frames("x"), before, "the delta's frame is painted");
    // Until the loop has been seen through a period again.
    for _ in 0..10 {
        t += fi;
        h.animate(t);
        render(&mut h);
    }
    assert_eq!(h.loop_frames("x"), before, "a period is watched first");
}

#[test]
fn a_change_away_from_the_loop_keeps_it() {
    let mut h = host();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x")],
        &format!(
            "{STYLE}<div class=bar><span class=dot></span></div><div id=s style='margin-left:150px'>one</div>"
        ),
    ));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "60"), ("r", "4")],
        "",
    ));
    render(&mut h);
    let fi = h.frame_interval();
    let per = (0.4 / fi.as_secs_f64()).round() as u32;
    let mut t = Instant::now() + AHEAD;
    let mut step = |h: &mut Host| {
        t += fi;
        h.animate(t);
        render(h);
        h.frame("x").unwrap().rgba.clone()
    };
    for _ in 0..80 {
        step(&mut h);
    }
    assert!(h.loop_frames("x") > 0);
    let before = h.loop_frames("x");
    h.handle(&common::delta_cmd(
        &[("s", "x")],
        "text",
        Some("s"),
        None,
        "two",
    ));
    step(&mut h);
    assert_eq!(h.loop_frames("x"), before, "the delta's frame is painted");
    let kept: Vec<Vec<u8>> = (0..per).map(|_| step(&mut h)).collect();
    assert_eq!(
        h.loop_frames("x") - before,
        u64::from(per),
        "the loop plays on from memory"
    );
    // Each as a full paint one period on paints it, the new text included.
    for (k, kept) in kept.iter().enumerate() {
        h.repaint();
        let painted = step(&mut h);
        let worst = kept
            .iter()
            .zip(&painted)
            .map(|(a, b)| (*a as i32 - *b as i32).abs())
            .max()
            .unwrap();
        assert!(worst <= 3, "frame {k}: {worst} off");
    }
}

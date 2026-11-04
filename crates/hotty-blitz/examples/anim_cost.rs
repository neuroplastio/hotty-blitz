//! What a CSS animation costs a host, without a terminal: a toast-sized
//! surface with a 3px countdown bar (`transform: scaleX(1→0)` over 60 s),
//! driven as a host drives it: sleep until `next_frame`, then
//! `render_dirty`. Prints wakes, frames delivered and damaged pixels per
//! second over the measured window. Count instructions around it with
//! `perf stat -D <warmup ms>`.
//!
//!   cargo run --release -p hotty-blitz --example anim_cost -- <steps|linear|static|none> [secs] [warmup]

use hotty::Message;
use hotty_blitz::{Config, Damage, Host, Metrics};
use std::time::{Duration, Instant};

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("steps");
    let secs: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10.0);
    let warm: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let timing = match mode {
        "steps" => "steps(600)",
        "linear" => "linear",
        "static" | "none" => "",
        // Any other timing function, as CSS writes it.
        t => t,
    };
    let anim = if timing.is_empty() {
        String::new()
    } else {
        format!("animation:drain 60s {timing} forwards;")
    };
    // `none`: the toast without its bar.
    let bar = if mode == "none" {
        String::new()
    } else {
        format!("<div class=bar style='{anim}'></div>")
    };
    let html = format!(
        "<style>
          body{{margin:0;font:14px sans-serif}}
          .toast{{position:absolute;right:16px;top:16px;width:300px;padding:12px 0 0;
                  background:#223;color:#eee;border-radius:8px;overflow:hidden}}
          .toast p{{margin:0 12px 12px}}
          .bar{{height:3px;width:300px;background:#6af;transform-origin:left}}
          @keyframes drain{{from{{transform:scaleX(1)}}to{{transform:scaleX(0)}}}}
        </style>
        <div class=toast><p>Build finished: 3 warnings</p>{bar}</div>"
    );
    // hottyterm on the headless display: scale 2, cells 20x42 (docs/performance.md).
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 20,
            cell_h: 42,
            scale: 2.0,
        },
        ..Config::default()
    });
    h.handle(&cmd(&[("a", "doc"), ("s", "t")], &html));
    h.handle(&cmd(
        &[("a", "place"), ("s", "t"), ("c", "60"), ("r", "4")],
        "",
    ));
    let start = Instant::now();
    let (mut wakes, mut frames, mut px, mut full) = (0u64, 0u64, 0u64, 0u64);
    let (mut minpx, mut maxpx) = (u64::MAX, 0u64);
    let mut measuring = false;
    let mut t_meas = start;
    loop {
        let now = Instant::now();
        let el = now.duration_since(start).as_secs_f64();
        if !measuring && el >= warm {
            measuring = true;
            t_meas = now;
        }
        if el >= warm + secs {
            break;
        }
        let mut got = Vec::new();
        h.render_dirty(&mut |_, f, d| {
            let p = match d {
                Damage::Full => (f.width as u64 * f.height as u64, true),
                Damage::Rects(rs) => (rs.iter().map(|r| r.w as u64 * r.h as u64).sum(), false),
            };
            got.push(p);
        });
        if measuring {
            wakes += 1;
            for (p, f) in &got {
                frames += 1;
                px += p;
                full += *f as u64;
                minpx = minpx.min(*p);
                maxpx = maxpx.max(*p);
            }
        }
        // Sleep until the host's next frame, as a terminal's timer does;
        // with nothing due, until the window ends.
        let end = start + Duration::from_secs_f64(warm + secs);
        let due = h.next_frame().unwrap_or(end).min(end);
        let now = Instant::now();
        if due > now {
            std::thread::sleep(due - now);
        }
    }
    let span = Instant::now().duration_since(t_meas).as_secs_f64();
    println!(
        "mode={mode} secs={span:.2} wakes/s={:.1} frames/s={:.1} px/frame={} (min {} max {}) full={full}",
        wakes as f64 / span,
        frames as f64 / span,
        if frames > 0 { px / frames } else { 0 },
        if frames > 0 { minpx } else { 0 },
        maxpx,
    );
}

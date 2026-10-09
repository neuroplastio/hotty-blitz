//! Painting's threads. vello_cpu paints on a pool of worker threads for each
//! render context, so how many contexts a host keeps is how many threads it
//! keeps. A host that keeps them per surface keeps hundreds once a program
//! has shown a few dozen surfaces; a host keeps a few, whatever it shows.
//!
//! A file of its own: the test counts the process's threads, and the other
//! tests' would count with them.

use hotty_blitz::{Config, Host, Metrics};
use hotty_wire::Command;
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

fn threads() -> usize {
    std::fs::read_dir("/proc/self/task").map_or(0, |d| d.count())
}

/// The thread count once it has stopped falling: a dropped pool's threads
/// exit on their own time.
fn settled() -> usize {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut last = threads();
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        let now = threads();
        if now == last {
            return now;
        }
        last = now;
    }
    last
}

#[test]
fn many_surfaces_of_many_sizes_keep_a_few_painting_threads() {
    if !std::path::Path::new("/proc/self/task").exists() {
        return;
    }
    let before = threads();
    let mut h = host();
    // Forty surfaces, each drawn whole at a size of its own, then changed
    // in a part: every surface paints rectangles of several sizes.
    for i in 0..40 {
        let s = format!("s{i}");
        let c = (8 + i * 3).to_string();
        let r = (2 + i % 7).to_string();
        h.handle(&cmd(
            &[("a", "doc"), ("s", &s), ("q", "2")],
            &format!("<p id=p style=margin:0>surface {i}</p><div style=height:200px></div>"),
        ));
        h.handle(&cmd(
            &[("a", "place"), ("s", &s), ("c", &c), ("r", &r), ("q", "2")],
            "",
        ));
        h.render_dirty(&mut |_, _, _| {});
        h.handle(&cmd(
            &[
                ("a", "delta"),
                ("s", &s),
                ("op", "text"),
                ("t", "p"),
                ("q", "2"),
            ],
            &format!("changed {i}"),
        ));
        h.render_dirty(&mut |_, _, _| {});
    }
    let kept = settled().saturating_sub(before);
    // A few contexts' pools, at most eight threads each, and the fetcher's.
    assert!(kept <= 72, "painting keeps {kept} threads");
}

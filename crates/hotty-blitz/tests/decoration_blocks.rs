use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

#[test]
fn decoration_crosses_block_but_not_atomic_inline_boundary() {
    let mut host = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    let cmd = |pairs: &[(&str, &str)], payload: &str| {
        Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
    };
    host.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], "<style>body{margin:0;background:white;font:24px/40px sans-serif;color:black}.outer{text-decoration:underline 3px red}.atomic{display:inline-block}</style><div class=outer><div>Block text</div><span class=atomic>Atomic text</span></div>"));
    host.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "20"),
            ("r", "4"),
            ("q", "2"),
        ],
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    let frame = host.frame("x").unwrap();
    let red = |top: usize, bottom: usize| {
        (top..bottom)
            .flat_map(|y| {
                (0..frame.width as usize).map(move |x| (y * frame.width as usize + x) * 4)
            })
            .filter(|&i| frame.rgba[i] > 200 && frame.rgba[i + 1] < 60 && frame.rgba[i + 2] < 60)
            .count()
    };
    assert!(red(0, 40) > 20, "decoration must reach the block child");
    assert_eq!(red(40, 80), 0, "decoration must stop at atomic inline");
}

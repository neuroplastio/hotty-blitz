use hotty::Message;
use hotty_blitz::{Config, Host, Metrics, Mods};

#[test]
fn large_repeating_gradient_draws_visible_tiles() {
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
    host.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2"), ("scroll", "1")], "<style>body{margin:0;background:white}.dots{width:200px;height:10000px;background:radial-gradient(red 2px,transparent 3px) 0 0 / 8px 8px}</style><div class=dots></div>"));
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
    let red = frame
        .rgba
        .chunks_exact(4)
        .filter(|p| p[0] > 200 && p[1] < 60 && p[2] < 60)
        .count();
    assert!(
        red > 100,
        "the visible dot tiles must render, got {red} red pixels"
    );
    assert!(
        host.wheel("x", 50.0, 40.0, 0.0, 400.0, Mods::default())
            .taken
    );
    host.end_gesture();
    host.render_dirty(&mut |_, _, _| {});
    let red_after_scroll = host
        .frame("x")
        .unwrap()
        .rgba
        .chunks_exact(4)
        .filter(|p| p[0] > 200 && p[1] < 60 && p[2] < 60)
        .count();
    assert!(
        red_after_scroll > 100,
        "newly visible dots must render, got {red_after_scroll} red pixels"
    );
}

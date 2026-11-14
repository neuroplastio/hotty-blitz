use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

#[test]
fn filter_layer_includes_overflowing_positioned_children() {
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
    host.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], "<style>body{margin:0;background:white}.parent{width:20px;height:20px;filter:blur(0)}.child{position:absolute;left:50px;top:0;width:20px;height:20px;background:red}</style><div class=parent><div class=child></div></div>"));
    host.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "12"),
            ("r", "3"),
            ("q", "2"),
        ],
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    let f = host.frame("x").unwrap();
    let i = ((10 * f.width + 60) * 4) as usize;
    assert_eq!(&f.rgba[i..i + 3], &[255, 0, 0]);
}

#[test]
fn opacity_layer_includes_overflowing_children() {
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
    host.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], "<style>body{margin:0;background:white}.parent{width:20px;height:20px;opacity:.5}.child{margin-left:50px;width:20px;height:20px;background:red}</style><div class=parent><div class=child></div></div>"));
    host.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "12"),
            ("r", "3"),
            ("q", "2"),
        ],
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    let f = host.frame("x").unwrap();
    let i = ((10 * f.width + 60) * 4) as usize;
    assert!(
        (120..=135).contains(&f.rgba[i + 1]),
        "half-opacity red beyond the parent: {:?}",
        &f.rgba[i..i + 3]
    );
}

#[test]
#[ignore = "Leg 5: positioned descendants need to paint inside their opacity ancestor stacking context"]
fn opacity_layer_includes_positioned_descendant_with_outer_containing_block() {
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
    host.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], "<style>body{margin:0;background:white}.outer{position:relative}.parent{width:20px;height:20px;opacity:.5}.child{position:absolute;left:80px;top:0;width:20px;height:20px;background:red}</style><div class=outer><div class=parent><div class=child></div></div></div>"));
    host.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "12"),
            ("r", "3"),
            ("q", "2"),
        ],
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    let f = host.frame("x").unwrap();
    let i = ((10 * f.width + 90) * 4) as usize;
    assert!(
        (120..=135).contains(&f.rgba[i + 1]),
        "positioned descendant must be in the opacity layer: {:?}",
        &f.rgba[i..i + 3]
    );
}

mod common;

use common::delta_cmd;
use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

#[test]
fn range_paints_its_value_and_accent() {
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
    host.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], "<style>body{margin:0;background:white}input{display:block;width:100px;height:20px;margin:0;accent-color:red}</style><input id=a type=range min=0 max=100 value=0><input type=range min=0 max=100 value=100>"));
    host.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "15"),
            ("r", "3"),
            ("q", "2"),
        ],
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    let f = host.frame("x").unwrap();
    let red = |x: u32, y: u32| {
        let i = ((y * f.width + x) * 4) as usize;
        f.rgba[i] > 200 && f.rgba[i + 1] < 80 && f.rgba[i + 2] < 80
    };
    assert!(red(8, 10), "zero value thumb uses the accent at the left");
    assert!(
        red(92, 30),
        "maximum value thumb uses the accent at the right"
    );
    host.handle(&delta_cmd(
        &[("s", "x"), ("q", "2")],
        "attr",
        Some("a"),
        Some("value"),
        "100",
    ));
    host.render_dirty(&mut |_, _, _| {});
    let f = host.frame("x").unwrap();
    let i = ((10 * f.width + 92) * 4) as usize;
    assert!(
        f.rgba[i] > 200 && f.rgba[i + 1] < 80 && f.rgba[i + 2] < 80,
        "a value delta moves the thumb"
    );
    host.handle(&delta_cmd(
        &[("s", "x"), ("q", "2")],
        "unattr",
        Some("a"),
        Some("value"),
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    let f = host.frame("x").unwrap();
    let i = ((7 * f.width + 50) * 4) as usize;
    assert!(
        f.rgba[i] > 200 && f.rgba[i + 1] < 80 && f.rgba[i + 2] < 80,
        "removing value restores the midpoint thumb"
    );
}

#[test]
fn legacy_thumb_color_respects_media_scope() {
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
    host.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], "<style>body{margin:0;background:white}input{display:block;width:100px;height:20px;margin:0;accent-color:blue}@media(max-width:1px){input::-webkit-slider-thumb{background-color:red}}@media(min-width:2px){input::-moz-range-thumb{background-color:lime}}</style><input type=range value=0>"));
    host.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "15"),
            ("r", "3"),
            ("q", "2"),
        ],
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    let f = host.frame("x").unwrap();
    let i = ((10 * f.width + 8) * 4) as usize;
    assert!(
        f.rgba[i] < 80 && f.rgba[i + 1] > 200 && f.rgba[i + 2] < 80,
        "the matching media rule makes the thumb green: {:?}",
        &f.rgba[i..i + 3]
    );
}

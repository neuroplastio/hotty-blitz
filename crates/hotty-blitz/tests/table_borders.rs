use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

#[test]
fn collapsed_border_does_not_cross_a_spanning_cell() {
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
    host.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], "<style>body{margin:0;background:white}table{border-collapse:collapse;table-layout:fixed;width:200px}td{border:4px solid red;height:40px;padding:0}</style><table><tr><td colspan=2></td></tr><tr><td></td><td></td></tr></table>"));
    host.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "25"),
            ("r", "6"),
            ("q", "2"),
        ],
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    let f = host.frame("x").unwrap();
    let pixel = |x, y| {
        let i = ((y * f.width + x) * 4) as usize;
        [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
    };
    assert_eq!(
        pixel(50, 1),
        [255, 0, 0],
        "cell borders form the outer top edge"
    );
    assert_eq!(
        pixel(1, 20),
        [255, 0, 0],
        "cell borders form the outer left edge"
    );
    assert_eq!(
        pixel(100, 20),
        [255, 255, 255],
        "no vertical border inside colspan"
    );
    assert_eq!(
        pixel(100, 60),
        [255, 0, 0],
        "the second row keeps its shared border"
    );
    let border_width = (90..110).filter(|&x| pixel(x, 60) == [255, 0, 0]).count();
    assert_eq!(
        border_width, 4,
        "the shared border keeps its authored width"
    );
}

#[test]
fn collapsed_border_does_not_cross_a_row_spanning_cell() {
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
    host.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], "<style>body{margin:0;background:white}table{border-collapse:collapse;table-layout:fixed;width:200px}td{border:4px solid red;height:40px;padding:0}</style><table><tr><td rowspan=2></td><td></td></tr><tr><td></td></tr></table>"));
    host.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "25"),
            ("r", "6"),
            ("q", "2"),
        ],
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    let f = host.frame("x").unwrap();
    let pixel = |x, y| {
        let i = ((y * f.width + x) * 4) as usize;
        [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
    };
    assert_eq!(
        pixel(50, 40),
        [255, 255, 255],
        "no horizontal border inside rowspan"
    );
    let border_width = (35..55).filter(|&y| pixel(150, y) == [255, 0, 0]).count();
    assert_eq!(border_width, 4, "the adjacent cells keep one shared border");
}

#[test]
fn shared_translucent_collapsed_border_is_painted_once() {
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
    host.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], "<style>body{margin:0;background:white}table{border-collapse:collapse;table-layout:fixed;width:200px}td{border:4px solid rgba(255,0,0,.5);height:40px;padding:0}</style><table><tr><td></td><td></td></tr><tr><td></td><td></td></tr></table>"));
    host.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "25"),
            ("r", "6"),
            ("q", "2"),
        ],
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    let f = host.frame("x").unwrap();
    let pixel = |x, y| {
        let i = ((y * f.width + x) * 4) as usize;
        [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
    };
    let outer = pixel(50, 1);
    assert!(
        outer[1] > 100 && outer[1] < 155,
        "fixture must be translucent: {outer:?}"
    );
    assert_eq!(
        pixel(100, 20),
        outer,
        "shared vertical edge is painted once"
    );
    assert_eq!(
        pixel(50, 44),
        outer,
        "shared horizontal edge is painted once"
    );
    assert_eq!(pixel(100, 44), outer, "the shared junction is painted once");
}

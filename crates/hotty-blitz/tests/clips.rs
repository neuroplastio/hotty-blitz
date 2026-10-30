//! Clipping: every element that clips its content clips it, however many
//! there are (mods/blitz/layer-depth.md).

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// 1,100 boxes side by side, each 4 px square with `overflow: hidden` and a
/// child 8 px tall: each box shows the child's top half, and nothing of the
/// bottom half. Blitz counted every layer it pushed in a paint against its
/// limit of 1,024, so the boxes after the 1,024th painted their children
/// whole. plx's dialog box, one clipped element per cell, lost its clips
/// from row 16 down.
#[test]
fn more_clipped_elements_than_the_layer_limit_all_clip() {
    const BOXES: usize = 1100;
    let html = format!(
        "<html><style>html,body{{margin:0;background:transparent}}
        .g{{display:flex;flex-wrap:wrap;width:800px}}
        .b{{width:4px;height:4px;margin:0 4px 4px 0;overflow:hidden}}
        .b>i{{display:block;width:4px;height:8px;background:#f00}}</style>
        <div class=g>{}</div></html>",
        "<div class=b><i></i></div>".repeat(BOXES)
    );
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], &html));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "80"),
            ("r", "5"),
            ("q", "2"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    let f = h.frame("x").unwrap();
    let w = f.width as usize;
    // Rows of boxes every 8 px: a box's 4 px, then its 4 px margin, where
    // only an unclipped child's bottom half could paint.
    let (mut inside, mut outside) = (0, 0);
    for (i, p) in f.rgba.chunks(4).enumerate() {
        if p[3] == 0 || p[0] < 128 {
            continue;
        }
        if (i / w) % 8 < 4 {
            inside += 1;
        } else {
            outside += 1;
        }
    }
    assert_eq!(outside, 0, "a child painted past its clipping box");
    assert_eq!(inside, BOXES * 16, "every box shows its child's top half");
}

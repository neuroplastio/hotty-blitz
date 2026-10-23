//! An inline-block sits on its line by its baseline however often it is
//! measured. Blitz's layout cache answered a repeated measurement with the
//! size alone, so an inline-block measured from the cache had no baseline
//! and sat on the line by its bottom edge: the line grew, and a flex item of
//! keycaps outgrew its row, which centred the text beside it lower than in
//! the rows around it.

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

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

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// The bounding box of what the surface draws in `colour`: (x0, y0, x1, y1),
/// ends exclusive.
fn bounds(h: &Host, colour: [u8; 3]) -> (u32, u32, u32, u32) {
    let f = h.frame("x").unwrap();
    let mut b = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..f.height {
        for x in 0..f.width {
            let i = ((y * f.width + x) * 4) as usize;
            if f.rgba[i..i + 3] == colour {
                b = (b.0.min(x), b.1.min(y), b.2.max(x + 1), b.3.max(y + 1));
            }
        }
    }
    assert!(b.0 < b.2, "nothing drawn in {colour:?}");
    b
}

const RED: [u8; 3] = [255, 0, 0];
const BLUE: [u8; 3] = [0, 0, 255];

/// A blue row in a column, wrapping: a red item of three inline-blocks that
/// don't fit its share, so it is held at its min-content width, and a text.
/// The column's sizing measures the red item twice under the same
/// constraints, and the second time its inline-blocks come from the cache.
#[test]
fn inline_blocks_measured_from_the_cache_keep_their_baselines() {
    let mut h = host();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x")],
        "<style>body{margin:0;font:20px/1.5 sans-serif;color:#fff;background:#000}\
         .c{display:flex;flex-direction:column}\
         .r{display:flex;flex-wrap:wrap;background:#00f}\
         .k{flex:1 1 0%;white-space:nowrap;background:#f00}\
         .t{flex:2 1 0%}\
         b{display:inline-block;line-height:1.3;border:1px solid #fff;font-weight:normal}</style>\
         <div class=c><div class=r>\
           <div class=k><b>AAAA</b> <b>BBBB</b> <b>CCCC</b></div><div class=t>x</div>\
         </div></div>",
    ));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "30"), ("r", "6")],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    let (_, ry0, _, ry1) = bounds(&h, RED);
    let (_, by0, _, by1) = bounds(&h, BLUE);
    assert_eq!(
        (ry0, ry1),
        (by0, by1),
        "the item of inline-blocks is as tall as its row"
    );
    assert_eq!(
        ry1 - ry0,
        30,
        "one line of 30px, the 28px inline-blocks on its baseline"
    );
}

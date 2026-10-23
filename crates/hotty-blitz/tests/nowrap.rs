//! Inline-blocks follow the `white-space` of the element they sit in, as in
//! a browser: in a `nowrap` element they stay on one line, and the element's
//! min-content width holds them all. Parley took a box's wrap mode from the
//! text before it, so a `nowrap` flex item of keycaps was as narrow as one
//! cap, and the caps were broken between and overflowed into its neighbour.

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

/// A surface of 30 by 6 cells showing `body`, where each `<b>` is a red
/// inline-block 40px by 20px.
fn show(body: &str) -> Host {
    let mut h = host();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x")],
        &format!(
            "<style>body{{margin:0;font:20px sans-serif;color:#fff;background:#000}}\
             b{{display:inline-block;width:40px;height:20px;background:#f00}}</style>{body}"
        ),
    ));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "30"), ("r", "6")],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    h
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

#[test]
fn a_nowrap_flex_item_of_inline_blocks_holds_them_on_one_line() {
    let h = show(
        "<div style='display:flex;width:200px'>\
           <div style='flex:1 1 0%;white-space:nowrap'><b></b><b></b><b></b></div>\
           <div style='flex:3 1 0%;height:20px;background:#00f'></div>\
         </div>",
    );
    let (x0, y0, x1, y1) = bounds(&h, RED);
    assert_eq!((x1 - x0, y1 - y0), (120, 20), "the three caps in a row");
    let (blue, ..) = bounds(&h, BLUE);
    assert_eq!(blue, 120, "the neighbour starts after the caps");
}

#[test]
fn inline_blocks_in_a_nowrap_span_move_to_the_next_line_together() {
    let h = show(
        "<div style='width:90px'>A <span style='white-space:nowrap'><b></b><b></b></span></div>",
    );
    let (x0, y0, x1, y1) = bounds(&h, RED);
    assert_eq!(x0, 0, "the caps start the second line");
    assert!(y0 >= 20, "the caps are below the A, from {y0}px");
    assert_eq!((x1 - x0, y1 - y0), (80, 20), "both caps on that line");
}

#[test]
fn inline_blocks_in_a_span_that_wraps_wrap_inside_a_nowrap_element() {
    let h = show(
        "<div style='width:70px;white-space:nowrap'>A<span style='white-space:normal'>\
           <b></b><b></b><b></b></span></div>",
    );
    let (_, y0, _, y1) = bounds(&h, RED);
    assert!(y1 - y0 >= 60, "a cap per line, not {}px of caps", y1 - y0);
}

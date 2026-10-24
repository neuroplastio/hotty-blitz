//! `text-overflow: ellipsis` on an element that clips its overflow: a line
//! that runs past the end edge stops after its last whole cluster or
//! inline-block that leaves room for the ellipsis, and the ellipsis follows
//! it, inside the content box. Blitz had no ellipsis, so such a line was
//! cut mid-glyph at the edge.

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn host(scale: f32) -> Host {
    Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale,
        },
        ..Config::default()
    })
}

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// A surface of 30 by 6 cells showing `body` in green, where each `<b>` is
/// a red inline-block 40px by 20px, and each `<i>` a blue one.
fn show(scale: f32, body: &str) -> Host {
    let mut h = host(scale);
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x")],
        &format!(
            "<style>body{{margin:0;font:40px sans-serif;color:#0f0;background:#000}}\
             b,i{{display:inline-block;width:40px;height:20px;background:#f00}}\
             i{{background:#00f}}</style>{body}"
        ),
    ));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "30"), ("r", "6")],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    h
}

/// The bounding box of what the surface draws in `colour`, in CSS px:
/// (x0, y0, x1, y1), ends exclusive. `None` when nothing is.
fn drawn(h: &Host, scale: f32, colour: [u8; 3]) -> Option<(f32, f32, f32, f32)> {
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
    (b.0 < b.2).then(|| {
        let px = |v: u32| v as f32 / scale;
        (px(b.0), px(b.1), px(b.2), px(b.3))
    })
}

const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];

const ELLIPSIS: &str = "white-space:nowrap;overflow:hidden;text-overflow:ellipsis";

#[test]
fn the_ellipsis_follows_the_last_inline_block_that_leaves_room_for_it() {
    for scale in [1.0, 2.0] {
        let h = show(
            scale,
            &format!(
                "<div style='{ELLIPSIS};width:130px;padding-right:20px'>\
                   <b></b><b></b><b></b><b></b></div>"
            ),
        );
        let (x0, _, x1, _) = drawn(&h, scale, RED).unwrap();
        assert_eq!(
            (x0, x1),
            (0.0, 80.0),
            "two whole caps at {scale}x, the third hidden"
        );
        let (e0, _, e1, _) = drawn(&h, scale, GREEN).expect("an ellipsis");
        assert!(
            e0 >= 80.0,
            "the ellipsis starts after the caps at {scale}x, not {e0}px"
        );
        assert!(
            e1 <= 130.0,
            "the ellipsis ends inside the content box at {scale}x, not {e1}px"
        );
    }
}

#[test]
fn a_line_that_fits_or_clips_shows_no_ellipsis() {
    let fits = show(
        1.0,
        &format!("<div style='{ELLIPSIS};width:110px'><b></b><b></b></div>"),
    );
    assert_eq!(
        drawn(&fits, 1.0, GREEN),
        None,
        "no ellipsis when the line fits"
    );
    let clips = show(
        1.0,
        "<div style='white-space:nowrap;overflow:hidden;width:110px'><b></b><b></b><b></b></div>",
    );
    assert_eq!(
        drawn(&clips, 1.0, GREEN),
        None,
        "no ellipsis under text-overflow: clip"
    );
    let (_, _, x1, _) = drawn(&clips, 1.0, RED).unwrap();
    assert_eq!(x1, 110.0, "clip cuts the third cap at the edge");
}

#[test]
fn only_the_line_that_overflows_gets_an_ellipsis() {
    let h = show(
        1.0,
        "<div style='overflow:hidden;text-overflow:ellipsis;width:130px'>\
           <b></b><br><span style='white-space:nowrap'><i></i><i></i><i></i><i></i></span><br><b></b>\
         </div>",
    );
    let (_, y0, _, y1) = drawn(&h, 1.0, GREEN).expect("an ellipsis");
    let (_, top, _, baseline) = drawn(&h, 1.0, BLUE).unwrap();
    assert!(
        y0 >= top && y1 <= baseline + 1.0,
        "one ellipsis, on the second line's baseline ({top}..{baseline}px), not {y0}..{y1}px"
    );
}

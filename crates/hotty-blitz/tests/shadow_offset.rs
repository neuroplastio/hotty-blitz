//! An inset box-shadow's offset is at the device scale, as an outset
//! one's is (mods/blitz/box-shadow.md).

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// `inset 8px 8px 0 #000` in a white 300 px square box at 100,100 CSS px on
/// a white page, at `scale`: the shadow's depth inside the box's top edge,
/// down its middle column, and inside its left edge, along its middle row,
/// in device pixels (the sum of the shadow's alpha).
fn bands(scale: f32) -> (f64, f64) {
    let html = "<html><style>html,body{margin:0;background:#fff}
        .b{position:absolute;left:100px;top:100px;width:300px;height:300px;
        background:#fff;box-shadow:inset 8px 8px 0 #000}</style><div class=b></div></html>";
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: (10.0 * scale) as u32,
            cell_h: (20.0 * scale) as u32,
            scale,
        },
        ..Config::default()
    });
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], html));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "60"),
            ("r", "25"),
            ("q", "2"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    let f = h.frame("x").unwrap();
    let w = f.width as usize;
    let alpha = |x: usize, y: usize| 1.0 - f.rgba[(y * w + x) * 4 + 1] as f64 / 255.0;
    let (edge, mid) = ((100.0 * scale) as usize, (250.0 * scale) as usize);
    let depth = |at: &dyn Fn(usize) -> f64| (0..40).map(|i| at(edge + i)).sum::<f64>();
    (depth(&|y| alpha(mid, y)), depth(&|x| alpha(x, mid)))
}

/// 8px is 8 device pixels at 1 and 12.8 at 1.6, to a pixel: Blitz moved
/// the hole 8 device pixels at any scale, 5 CSS px at 1.6.
#[test]
fn an_inset_shadow_s_offset_is_at_the_device_scale() {
    for scale in [1.0f32, 1.6] {
        let want = 8.0 * scale as f64;
        let (top, left) = bands(scale);
        assert!(
            (top - want).abs() <= 1.0 && (left - want).abs() <= 1.0,
            "scale {scale}: the band is {top:.1} px deep at the top and {left:.1} at the left, CSS's {want:.1}"
        );
    }
}

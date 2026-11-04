//! Inset box-shadows without blur: the ring a hairline is drawn with
//! (mods/blitz/inset-ring.md).

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// One box at the top left of a black document, black itself, with the
/// given inset shadow in white, at 1.6: the frame's luma, row by row.
fn paint(shadow: &str) -> (usize, Vec<u8>) {
    let html = format!(
        "<html><style>html,body{{margin:0;background:#000}}
        .b{{width:100px;height:40px;border-radius:12px;background:#000;box-shadow:{shadow}}}
        </style><div class=b></div></html>"
    );
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 16,
            cell_h: 32,
            scale: 1.6,
        },
        ..Config::default()
    });
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], &html));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "20"),
            ("r", "4"),
            ("q", "2"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    let f = h.frame("x").unwrap();
    (f.width as usize, f.rgba.chunks(4).map(|p| p[1]).collect())
}

/// `inset 0 0 0 1px` at 1.6 is a ring one whole device pixel wide on each
/// straight edge: the spread rounded down to device pixels, as Stylo snaps
/// a 1px border to one, and nothing inside: the box is 160×64 device
/// pixels.
#[test]
fn a_hairline_inset_shadow_is_one_whole_pixel_at_1_6() {
    let (w, px) = paint("inset 0 0 0 1px #fff");
    let at = |x: usize, y: usize| px[y * w + x];
    for (x, y) in [(80, 0), (80, 63), (0, 32), (159, 32)] {
        assert_eq!(at(x, y), 255, "ring at {x},{y}");
    }
    for (x, y) in [(80, 1), (80, 62), (1, 32), (158, 32), (80, 32)] {
        assert_eq!(at(x, y), 0, "inside at {x},{y}");
    }
}

/// A spread past half the box leaves no hole: the whole padding box is
/// shadow, as CSS says. Blitz cut a hole out of the middle, the inverted
/// rectangle made positive.
#[test]
fn an_inset_spread_past_half_the_box_shadows_all_of_it() {
    let (w, px) = paint("inset 0 0 0 30px #fff");
    let at = |x: usize, y: usize| px[y * w + x];
    for (x, y) in [(80, 32), (40, 32), (120, 32), (80, 10), (80, 54)] {
        assert_eq!(at(x, y), 255, "shadow at {x},{y}");
    }
}

/// A spread of zero or less leaves the hole over the whole padding box: no
/// shadow at all. Blitz's layers left the rounded corners' anti-aliasing
/// behind, up to 38 of 255 at 1.6.
#[test]
fn an_inset_spread_of_zero_or_less_paints_nothing() {
    for shadow in ["inset 0 0 0 0 #fff", "inset 0 0 0 -2px #fff"] {
        let (_, px) = paint(shadow);
        let lit = px.iter().filter(|&&v| v > 0).count();
        assert_eq!(lit, 0, "{shadow}: {lit} lit of {}", px.len());
    }
}

/// Boxes at every eighth of a CSS pixel across and down, 40×24 CSS px,
/// black on black with a white `inset 0 0 0 1px` and `radius`, at `scale`:
/// for each box, the ink across each side through its middle (left, top,
/// right, bottom), in device pixels, and the most any pixel there is
/// neither ink nor background (0 when every pixel is whole).
fn hairline_grid(scale: f32, radius: &str) -> Vec<(String, [f32; 4], f32)> {
    const W: f32 = 40.0;
    const H: f32 = 24.0;
    let mut doc = format!(
        "<style>html,body{{margin:0;background:#000}}\
         .b{{position:absolute;width:{W}px;height:{H}px;background:#000;\
         border-radius:{radius};box-shadow:inset 0 0 0 1px #fff}}</style>"
    );
    let mut boxes = Vec::new();
    for j in 0..8 {
        for i in 0..8 {
            let x = 4.0 + 48.0 * i as f32 + i as f32 / 8.0;
            let y = 4.0 + 32.0 * j as f32 + j as f32 / 8.0;
            doc += &format!("<div class=b style='left:{x}px;top:{y}px'></div>");
            boxes.push((x, y));
        }
    }
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: (10.0 * scale).round() as u32,
            cell_h: (20.0 * scale).round() as u32,
            scale,
        },
        ..Config::default()
    });
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], &doc));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "42"), ("r", "14")],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    let f = h.frame("x").unwrap();
    let ink =
        |x: i32, y: i32| f.rgba[((y as u32 * f.width + x as u32) * 4 + 1) as usize] as f32 / 255.0;
    let dev = |v: f32| (v * scale).floor() as i32;
    boxes
        .iter()
        .map(|&(x, y)| {
            let (mx, my) = (dev(x + W / 2.0), dev(y + H / 2.0));
            let mut part = 0.0f32;
            let mut across = |(x0, y0): (i32, i32), (dx, dy): (i32, i32)| -> f32 {
                (0..9)
                    .map(|k| {
                        let v = ink(x0 + k * dx, y0 + k * dy);
                        part = part.max(v.min(1.0 - v));
                        v
                    })
                    .sum()
            };
            let sides = [
                across((dev(x) - 4, my), (1, 0)),
                across((mx, dev(y) - 4), (0, 1)),
                across((dev(x + W) - 5, my), (1, 0)),
                across((mx, dev(y + H) - 5), (0, 1)),
            ];
            (format!("{x},{y}"), sides, part)
        })
        .collect()
}

/// Every side of every box in the grid draws `want` whole device pixels.
/// The box sat at a fraction of a device pixel and only the hole's edge was
/// snapped, so at 1.6 a side drew 1.25 to 2.1 device pixels, the outer one
/// anti-aliased: hotty-surf's button had a left hairline a pixel thinner
/// than the others. Now the box is snapped as browsers snap it and the
/// spread is rounded down to whole device pixels, as a border's width is:
/// 1px is 1 device pixel below 2x, 2 at 2 and 2.5, 3 at 3.
fn every_side_whole(scale: f32, want: f32) {
    for radius in ["0", "4px"] {
        let bad: Vec<_> = hairline_grid(scale, radius)
            .into_iter()
            .filter(|(_, sides, part)| {
                *part > 0.02 || sides.iter().any(|s| (s - want).abs() > 0.05)
            })
            .map(|(at, s, part)| format!("{at}: {s:.2?} part {part:.2}"))
            .collect();
        assert!(
            bad.is_empty(),
            "at {scale}x, radius {radius}, {} of 64 boxes do not draw {want} whole device px a side: {}",
            bad.len(),
            bad.iter().take(6).cloned().collect::<Vec<_>>().join("; ")
        );
    }
}

#[test]
fn a_hairline_inset_shadow_is_one_whole_pixel_a_side_at_1x() {
    every_side_whole(1.0, 1.0);
}

#[test]
fn a_hairline_inset_shadow_is_one_whole_pixel_a_side_at_1_25x() {
    every_side_whole(1.25, 1.0);
}

#[test]
fn a_hairline_inset_shadow_is_one_whole_pixel_a_side_at_1_5x() {
    every_side_whole(1.5, 1.0);
}

#[test]
fn a_hairline_inset_shadow_is_one_whole_pixel_a_side_at_1_6x() {
    every_side_whole(1.6, 1.0);
}

#[test]
fn a_hairline_inset_shadow_is_two_whole_pixels_a_side_at_2x() {
    every_side_whole(2.0, 2.0);
}

#[test]
fn a_hairline_inset_shadow_is_two_whole_pixels_a_side_at_2_5x() {
    every_side_whole(2.5, 2.0);
}

#[test]
fn a_hairline_inset_shadow_is_three_whole_pixels_a_side_at_3x() {
    every_side_whole(3.0, 3.0);
}

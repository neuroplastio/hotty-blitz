//! A thin border draws on every side, wherever its box sits, at any scale.
//! Stylo snaps a border's width to whole device pixels: 1 CSS px is one
//! device pixel at 1.6x, 0.625 CSS px. Layout then rounded each edge of a
//! box to whole CSS pixels, the border's inner edges too, so such a border
//! came out 0 or 1 CSS px a side by the fraction of the box's position: a
//! side drew nothing, or 1.6 device pixels, and with a radius often no side
//! drew. hotty-surf's Compact fields lost their line, their error colour
//! and their focus accent at 1.6x, and a key cap its one-device-pixel
//! border at 1.6x and 2x.
//!
//! Each test lays out boxes at every eighth of a CSS pixel across and down,
//! red borders on black, and measures how much red each side draws across
//! its middle, in device pixels: as many as Stylo snapped the width to, on
//! every side alike, as Chromium draws it.

mod common;

use common::delta_cmd;
use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

/// A box's border box, in CSS px.
const W: f32 = 40.0;
const H: f32 = 24.0;
/// Where the boxes of the grid are, in CSS px: 8 columns and 8 rows, the
/// one in column `i` and row `j` an `i`/8 px right and a `j`/8 px down.
const PITCH_X: f32 = 48.0;
const PITCH_Y: f32 = 32.0;
const STEPS: usize = 8;

fn host(scale: f32) -> Host {
    Host::new(Config {
        metrics: Metrics {
            cell_w: (10.0 * scale).round() as u32,
            cell_h: (20.0 * scale).round() as u32,
            scale,
        },
        ..Config::default()
    })
}

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// `doc` placed on a surface big enough for the grid, and painted.
fn shown(scale: f32, doc: &str) -> Host {
    let mut h = host(scale);
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], doc));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "42"), ("r", "14")],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    h
}

/// The page: black, and `boxes` (id, a class beside `b`, left, top in CSS
/// px), each with a 1px red border, which `css` may restyle.
fn page(css: &str, boxes: &[(&str, &str, f32, f32)]) -> String {
    let mut doc = format!(
        "<style>html,body{{margin:0;background:#000}}\
         .b{{position:absolute;box-sizing:border-box;width:{W}px;height:{H}px;\
         border:1px solid #f00}}{css}</style>"
    );
    for &(id, class, x, y) in boxes {
        doc += &format!("<div id={id} class='b {class}' style='{}'></div>", at(x, y));
    }
    doc
}

fn at(x: f32, y: f32) -> String {
    format!("left:{x}px;top:{y}px")
}

/// The grid's boxes: id, left, top.
fn grid() -> Vec<(String, f32, f32)> {
    let mut boxes = Vec::new();
    for j in 0..STEPS {
        for i in 0..STEPS {
            let x = 4.0 + PITCH_X * i as f32 + i as f32 / 8.0;
            let y = 4.0 + PITCH_Y * j as f32 + j as f32 / 8.0;
            boxes.push((format!("g{i}{j}"), x, y));
        }
    }
    boxes
}

/// How red the device pixel at `x`, `y` is, 0 to 1.
fn red(h: &Host, x: i32, y: i32) -> f32 {
    let f = h.frame("x").unwrap();
    assert!(x >= 0 && y >= 0 && (x as u32) < f.width && (y as u32) < f.height);
    f.rgba[((y as u32 * f.width + x as u32) * 4) as usize] as f32 / 255.0
}

/// The red drawn across each side of the box at `x`, `y` (CSS px), through
/// its middle, in device pixels: left, top, right, bottom. Each window is 9
/// device pixels about the side, wide enough for a border of 3 wherever
/// layout rounds the box to (a whole CSS pixel, so a side can start at a
/// fraction of a device pixel), and far from the side across and from the
/// next box.
fn sides(h: &Host, scale: f32, x: f32, y: f32) -> [f32; 4] {
    let dev = |v: f32| (v * scale).floor() as i32;
    let (mx, my) = (dev(x + W / 2.0), dev(y + H / 2.0));
    let across = |(x0, y0): (i32, i32), (dx, dy): (i32, i32)| -> f32 {
        (0..9).map(|k| red(h, x0 + k * dx, y0 + k * dy)).sum()
    };
    [
        across((dev(x) - 4, my), (1, 0)),
        across((mx, dev(y) - 4), (0, 1)),
        across((dev(x + W) - 5, my), (1, 0)),
        across((mx, dev(y + H) - 5), (0, 1)),
    ]
}

const SIDE: [&str; 4] = ["left", "top", "right", "bottom"];

/// The sides of `boxes` in `h` that do not draw `want` device pixels of
/// red, each as "id side: drawn".
fn wrong(h: &Host, scale: f32, boxes: &[(String, f32, f32)], want: [f32; 4]) -> Vec<String> {
    let mut out = Vec::new();
    for (id, x, y) in boxes {
        let got = sides(h, scale, *x, *y);
        for k in 0..4 {
            if (got[k] - want[k]).abs() > 0.1 {
                out.push(format!("{id} at {x},{y} {}: {:.2}", SIDE[k], got[k]));
            }
        }
    }
    out
}

/// Every side of every box in the grid, `css` styling them, draws `want`
/// device pixels of red at `scale`.
fn every_side(scale: f32, css: &str, want: [f32; 4]) {
    let boxes = grid();
    let list: Vec<_> = boxes
        .iter()
        .map(|(id, x, y)| (id.as_str(), "", *x, *y))
        .collect();
    let h = shown(scale, &page(css, &list));
    let bad = wrong(&h, scale, &boxes, want);
    assert!(
        bad.is_empty(),
        "at {scale}x, {} of {} sides do not draw {want:?} device px: {}",
        bad.len(),
        4 * boxes.len(),
        bad.iter().take(8).cloned().collect::<Vec<_>>().join("; ")
    );
}

/// A 1px border: one device pixel up to 2x, where it is two.
const ONE: [f32; 4] = [1.0; 4];
const TWO: [f32; 4] = [2.0; 4];
const ROUND: &str = ".b{border-radius:4px}";
/// A key cap's: one device pixel, two at the bottom, and a radius.
const CAP: &str = ".b{border:var(--hotty-px) solid #f00;\
    border-bottom-width:calc(2 * var(--hotty-px));border-radius:5px}";
const CAP_SIDES: [f32; 4] = [1.0, 1.0, 1.0, 2.0];

#[test]
fn a_1px_border_draws_every_side_at_1x() {
    every_side(1.0, "", ONE);
}

#[test]
fn a_1px_border_draws_every_side_at_1_25x() {
    every_side(1.25, "", ONE);
}

#[test]
fn a_1px_border_draws_every_side_at_1_5x() {
    every_side(1.5, "", ONE);
}

#[test]
fn a_1px_border_draws_every_side_at_1_6x() {
    every_side(1.6, "", ONE);
}

#[test]
fn a_1px_border_draws_every_side_at_2x() {
    every_side(2.0, "", TWO);
}

#[test]
fn a_rounded_1px_border_draws_every_side_at_1x() {
    every_side(1.0, ROUND, ONE);
}

#[test]
fn a_rounded_1px_border_draws_every_side_at_1_25x() {
    every_side(1.25, ROUND, ONE);
}

#[test]
fn a_rounded_1px_border_draws_every_side_at_1_5x() {
    every_side(1.5, ROUND, ONE);
}

#[test]
fn a_rounded_1px_border_draws_every_side_at_1_6x() {
    every_side(1.6, ROUND, ONE);
}

#[test]
fn a_rounded_1px_border_draws_every_side_at_2x() {
    every_side(2.0, ROUND, TWO);
}

#[test]
fn a_key_caps_device_pixel_border_draws_every_side_at_1x() {
    every_side(1.0, CAP, CAP_SIDES);
}

#[test]
fn a_key_caps_device_pixel_border_draws_every_side_at_1_6x() {
    every_side(1.6, CAP, CAP_SIDES);
}

#[test]
fn a_key_caps_device_pixel_border_draws_every_side_at_2x() {
    every_side(2.0, CAP, CAP_SIDES);
}

/// A 2px border at 1.6x is 3.2 device pixels, which Stylo snaps down to 3,
/// as Chromium does, on every side wherever the box sits.
#[test]
fn a_2px_border_draws_three_device_pixels_every_side_at_1_6x() {
    every_side(1.6, ".b{border-width:2px}", [3.0; 4]);
}

/// Where the partial paint's square box (a) and rounded box (b, class r)
/// start.
const START: [(&str, &str, f32, f32); 2] = [("a", "", 10.0, 10.0), ("b", "r", 70.0, 10.0)];

/// Damage-only painting after deltas that move a square and a rounded box
/// by eighths of a pixel draws what one full paint of the same document
/// does, and both still draw every side.
#[test]
fn a_moved_box_paints_as_a_full_paint_does_at_1_6x() {
    let scale = 1.6;
    let doc = page(&ROUND.replace(".b", ".r"), &START);
    // Each move sets both boxes off their start by an eighth of a pixel or
    // more, across and down.
    let moves = [(0.125, 0.375), (0.5, 0.75), (0.875, 1.25), (1.625, 0.5)];
    let mut ops = Vec::new();
    for (dx, dy) in moves {
        for (id, _, x, y) in START {
            ops.push((id, at(x + dx, y + dy)));
        }
    }
    let mut inc = shown(scale, &doc);
    for (id, style) in &ops {
        inc.handle(&delta_cmd(
            &[("s", "x"), ("n", "1")],
            "attr",
            Some(id),
            Some("style"),
            style,
        ));
        inc.render_dirty(&mut |_, _, _| {});
    }
    let mut full = host(scale);
    full.handle(&cmd(&[("a", "doc"), ("s", "x")], &doc));
    full.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "42"), ("r", "14")],
        "",
    ));
    for (id, style) in &ops {
        full.handle(&delta_cmd(
            &[("s", "x"), ("n", "1")],
            "attr",
            Some(id),
            Some("style"),
            style,
        ));
    }
    full.render_dirty(&mut |_, _, _| {});
    let (a, b) = (inc.frame("x").unwrap(), full.frame("x").unwrap());
    let n = a
        .rgba
        .chunks(4)
        .zip(b.rgba.chunks(4))
        .filter(|(p, q)| p != q)
        .count();
    assert_eq!(n, 0, "{n} pixels differ from a full paint");
    let (dx, dy) = moves[moves.len() - 1];
    let boxes: Vec<_> = START
        .iter()
        .map(|&(id, _, x, y)| (id.to_string(), x + dx, y + dy))
        .collect();
    let bad = wrong(&inc, scale, &boxes, ONE);
    assert!(bad.is_empty(), "after the moves: {}", bad.join("; "));
}

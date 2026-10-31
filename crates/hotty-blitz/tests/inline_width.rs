//! A box sized by its inline content is as wide as that content, at any
//! scale: an inline-block holding inline-blocks of fractional widths is as
//! wide as a block given the same width, and a shrink-to-fit box of text
//! still holds its text on one line. Blitz rounded such a width up to a
//! whole device pixel, so that laying the box out again at it could not
//! break its last line, and rounding to CSS pixels then made it up to a
//! whole CSS pixel wider than its content: plx's runs of cells of a
//! fractional width came out ragged.

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

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

/// `doc` on a surface 80 cells by `rows`, painted.
fn shown(scale: f32, doc: &str, rows: u32) -> Host {
    let mut h = host(scale);
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], doc));
    let rows = rows.to_string();
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "80"), ("r", &rows)],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    h
}

/// How much of channel `c` (0 red, 1 green) device row `y` holds where the
/// other two channels are dark, in CSS px.
fn run(h: &Host, scale: f32, y: u32, c: usize) -> f32 {
    let f = h.frame("x").unwrap();
    let row = &f.rgba[(y * f.width * 4) as usize..((y + 1) * f.width * 4) as usize];
    row.chunks(4)
        .filter(|p| (0..3).all(|k| k == c || p[k] < 8))
        .map(|p| p[c] as f32 / 255.0)
        .sum::<f32>()
        / scale
}

/// Widths and counts of the inline-blocks: plx's cell widths at its
/// usual sizes, and a few counts of each.
const CASES: [(f32, u32); 12] = [
    (8.8, 3),
    (8.8, 9),
    (8.8, 37),
    (7.2, 7),
    (7.2, 13),
    (9.6, 21),
    (6.4, 9),
    (8.4, 13),
    (10.4, 7),
    (7.7, 21),
    (9.3, 37),
    (9.3, 3),
];

/// Each case's auto-width inline-block (red) and a block given its width
/// (green), each on a row 12 CSS px tall, differ by less than half a CSS
/// pixel, and the inline-block holds one line.
fn auto_matches_explicit(scale: f32) {
    let mut doc = String::from("<body style='margin:0;background:#000;width:700px'>");
    for (w, n) in CASES {
        doc += "<div style='height:12px'><span style='display:inline-block;background:#f00;line-height:0;vertical-align:top'>";
        for _ in 0..n {
            doc += &format!(
                "<i style='display:inline-block;width:{w}px;height:4px;vertical-align:top'></i>"
            );
        }
        doc += &format!(
            "</span></div><div style='height:12px'><div style='background:#0f0;height:4px;width:calc({n} * {w}px)'></div></div>"
        );
    }
    let h = shown(scale, &doc, 16);
    let mut bad = Vec::new();
    for (k, (w, n)) in CASES.iter().enumerate() {
        let ya = ((2 * k as u32 * 12 + 1) as f32 * scale) as u32;
        let yb = (((2 * k as u32 + 1) * 12 + 1) as f32 * scale) as u32;
        let (auto, explicit) = (run(&h, scale, ya, 0), run(&h, scale, yb, 1));
        let below = run(
            &h,
            scale,
            ((2 * k as u32 * 12 + 6) as f32 * scale) as u32,
            0,
        );
        if (auto - explicit).abs() >= 0.5 || below > 0.0 {
            bad.push(format!(
                "{n} x {w}px: auto {auto:.2}, explicit {explicit:.2}, below {below:.2}"
            ));
        }
    }
    assert!(bad.is_empty(), "at {scale}x: {}", bad.join("; "));
}

#[test]
fn an_auto_width_inline_block_is_as_wide_as_its_content_at_1x() {
    auto_matches_explicit(1.0);
}

#[test]
fn an_auto_width_inline_block_is_as_wide_as_its_content_at_1_25x() {
    auto_matches_explicit(1.25);
}

#[test]
fn an_auto_width_inline_block_is_as_wide_as_its_content_at_1_6x() {
    auto_matches_explicit(1.6);
}

#[test]
fn an_auto_width_inline_block_is_as_wide_as_its_content_at_2x() {
    auto_matches_explicit(2.0);
}

/// Shrink-to-fit boxes of one line of text: inline-block, float, flex item
/// and absolute, each red with transparent text, 10 CSS px a line.
const TEXTS: [&str; 6] = [
    "a b c d e f",
    "Hello world",
    "x 1.25 y 1.6",
    "Résumé café",
    "WWW iii",
    "mmm nnn",
];

fn text_stays_on_one_line(scale: f32) {
    let style =
        |f: f32| format!("background:#f00;color:transparent;line-height:10px;font-size:{f}px");
    let mut doc = String::from("<body style='margin:0;background:#000;width:700px'>");
    let mut cases = Vec::new();
    for f in [11.0, 13.7, 15.3] {
        for t in TEXTS {
            let s = style(f);
            for (kind, row) in [
                (
                    "inline-block",
                    format!(
                        "<div style='height:24px'><span style='display:inline-block;{s}'>{t}</span></div>"
                    ),
                ),
                (
                    "float",
                    format!(
                        "<div style='height:24px;overflow:hidden'><div style='float:left;{s}'>{t}</div></div>"
                    ),
                ),
                (
                    "flex item",
                    format!(
                        "<div style='height:24px;display:flex;align-items:flex-start'><div style='{s}'>{t}</div></div>"
                    ),
                ),
                (
                    "absolute",
                    format!(
                        "<div style='height:24px;position:relative'><div style='position:absolute;left:0;top:0;{s}'>{t}</div></div>"
                    ),
                ),
            ] {
                doc += &row;
                cases.push(format!("{kind} {f}px {t:?}"));
            }
        }
    }
    let rows = (cases.len() as u32 * 24).div_ceil(20) + 1;
    let h = shown(scale, &doc, rows);
    let f = h.frame("x").unwrap();
    let x = scale as u32;
    let mut bad = Vec::new();
    for (k, case) in cases.iter().enumerate() {
        let (y0, y1) = (
            (k as f32 * 24.0 * scale) as u32,
            ((k + 1) as f32 * 24.0 * scale) as u32,
        );
        let tall = (y0..y1.min(f.height))
            .map(|y| &f.rgba[((y * f.width + x) * 4) as usize..][..4])
            .filter(|p| p[1] < 8)
            .map(|p| p[0] as f32 / 255.0)
            .sum::<f32>()
            / scale;
        if tall > 12.0 {
            bad.push(format!("{case}: {tall:.1} CSS px tall"));
        }
    }
    assert!(bad.is_empty(), "at {scale}x, broke: {}", bad.join("; "));
}

#[test]
fn shrink_to_fit_text_stays_on_one_line_at_1_1x() {
    text_stays_on_one_line(1.1);
}

#[test]
fn shrink_to_fit_text_stays_on_one_line_at_1_6x() {
    text_stays_on_one_line(1.6);
}

#[test]
fn shrink_to_fit_text_stays_on_one_line_at_1_75x() {
    text_stays_on_one_line(1.75);
}

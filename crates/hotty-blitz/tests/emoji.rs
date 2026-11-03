//! An emoji draws in colour whatever font the document asks for, as a
//! browser draws it: a cluster with emoji presentation takes a colour font
//! before a text font that has outlines for it (the parley patch,
//! mods/parley/colour-emoji-first.md). A text-presentation cluster stays
//! text.

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

/// Whether the text, drawn grey at 40px in `family`, puts any strongly
/// coloured pixel on the surface: grey text has none, a colour emoji many.
fn coloured(text: &str, family: &str) -> bool {
    let mut h = host();
    let html = format!(
        "<style>body{{margin:0;background:#000}}</style>\
         <div style=\"font: 40px/48px {family}; color: #888\">{text}</div>"
    );
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], &html));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "20"),
            ("r", "3"),
            ("q", "2"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    let f = h.frame("x").unwrap();
    f.rgba.chunks_exact(4).any(|p| {
        let (hi, lo) = (p[0].max(p[1]).max(p[2]), p[0].min(p[1]).min(p[2]));
        hi - lo > 48
    })
}

#[test]
fn an_emoji_draws_in_colour_whatever_the_font_stack() {
    if !coloured("😀", "emoji") {
        eprintln!("skipped: no colour emoji font on this system");
        return;
    }
    // DejaVu Sans and the monospace generic have outlines for these faces;
    // they won before.
    for family in ["monospace", "sans-serif", "'DejaVu Sans', monospace"] {
        for text in ["😀", "🙂", "⚡", "❤️", "🇩🇪", "👍🏽", "🏳️‍🌈"] {
            assert!(coloured(text, family), "{text} in {family}");
        }
    }
}

#[test]
fn text_presentation_stays_text() {
    if !coloured("😀", "emoji") {
        eprintln!("skipped: no colour emoji font on this system");
        return;
    }
    for text in ["☺︎", "❤", "1", "#", "©", "abc"] {
        assert!(!coloured(text, "sans-serif"), "{text}");
    }
}

/// A red disc emoji (U+1F534) drawn at `px` on a black line: each of the
/// line's pixels' red as a share of the brightest, row by row, and the
/// frame's width.
fn disc(px: u32) -> (Vec<f32>, usize) {
    let mut h = host();
    let html = format!(
        "<style>body{{margin:0;background:#000}}</style>\
         <div style=\"font: {px}px/{line}px emoji; color: #888\">\u{1F534}</div>",
        line = px + 8
    );
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], &html));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "8"),
            ("r", "3"),
            ("q", "2"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    let f = h.frame("x").unwrap();
    // The line only: below it is the host's background, not black.
    let line = (px as usize + 8) * f.width as usize * 4;
    let red: Vec<f32> = f.rgba[..line]
        .chunks_exact(4)
        .map(|p| f32::from(p[0]))
        .collect();
    let max = red.iter().copied().fold(0.0, f32::max).max(1.0);
    (red.iter().map(|r| r / max).collect(), f.width as usize)
}

/// A colour emoji is drawn from a strike several times its size: shrunk by
/// averaging (mods/glifo/shrink-bitmap-glyphs.md), its round edge is a band
/// of part-covered pixels, as a browser's is, rather than a staircase of
/// pixels either in or out. Sampled from the strike, the disc had 15 to 20
/// steps from out straight to in round its edge; shrunk by averaging, it
/// has 0 to 3, where the edge runs along the pixel grid.
#[test]
fn a_shrunk_emoji_has_soft_edges() {
    if !coloured("😀", "emoji") {
        eprintln!("skipped: no colour emoji font on this system");
        return;
    }
    for px in [14, 20, 28] {
        let (cov, w) = disc(px);
        let partial = cov.iter().filter(|c| **c > 0.1 && **c < 0.9).count();
        let full = cov.iter().filter(|c| **c >= 0.9).count();
        assert!(full > 0 && partial > 0, "{px}px: no disc");
        // Neighbours, across or down, of which one is out and one is in.
        let mut steps = 0;
        for (i, c) in cov.iter().enumerate() {
            for j in [i + 1, i + w] {
                if (j == i + w || j % w != 0)
                    && let Some(d) = cov.get(j)
                    && c.min(*d) < 0.1
                    && c.max(*d) > 0.9
                {
                    steps += 1;
                }
            }
        }
        assert!(
            steps <= 4,
            "{px}px: {steps} steps from out to in ({full} in, {partial} part)"
        );
    }
}

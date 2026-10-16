//! An emoji draws in colour whatever font the document asks for, as a
//! browser draws it: a cluster with emoji presentation takes a colour font
//! before a text font that has outlines for it (the parley fork,
//! parley/README.md). A text-presentation cluster stays text.

use hotty_blitz::{Config, Host, Metrics};
use hotty_wire::Command;

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

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Command {
    Command::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
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
    h.handle(&cmd(&[("a", "place"), ("s", "x"), ("c", "20"), ("r", "3"), ("q", "2")], ""));
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

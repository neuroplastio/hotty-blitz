//! `field-sizing: content` on a textarea: it is as tall as its text, from
//! `min-height` to `max-height`, and scrolls past the most.

mod common;

use common::body;
use hotty::Message;
use hotty::host::{Output, Splitter};
use hotty_blitz::{Config, Effect, Host, Metrics};

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// A surface with `style` on a textarea of 20px lines, 100px wide, and a
/// 10px marker under it, whose top is the textarea's bottom.
fn show(style: &str, value: &str, caret: usize) -> Host {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x"), ("q", "2")],
        &format!(
            "<style>body{{margin:0}}\
             textarea{{display:block;margin:0;padding:0;border:0;width:100px;\
             font:16px monospace;line-height:20px;color:#000;background:#fff;{style}}}\
             #m{{height:10px}}</style><textarea id=t></textarea><div id=m></div>"
        ),
    ));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "30"),
            ("r", "20"),
            ("q", "2"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    h.set_text_field("x", "t", value, caret, caret);
    h.render_dirty(&mut |_, _, _| {});
    h
}

/// The textarea's height: where the marker starts.
fn height(h: &Host) -> f32 {
    h.element_centre("x", "m").unwrap().1 - 5.0
}

const GROWS: &str = "field-sizing:content;min-height:2lh;max-height:5lh;overflow-y:auto";

#[test]
fn a_textarea_grows_with_its_lines_from_min_to_max() {
    for (value, rows) in [
        ("", 2.0),
        ("a", 2.0),
        ("a\nb", 2.0),
        ("a\nb\nc", 3.0),
        ("a\nb\nc\nd\ne", 5.0),
        ("a\nb\nc\nd\ne\nf\ng", 5.0),
    ] {
        let h = show(GROWS, value, 0);
        assert_eq!(height(&h), rows * 20.0, "{value:?}");
    }
}

#[test]
fn a_textarea_grows_with_the_lines_its_text_wraps_to() {
    // 100px is 12 characters of 16px monospace (about 9.6px each).
    let h = show(GROWS, &"word ".repeat(12), 0);
    assert_eq!(height(&h), 5.0 * 20.0);
    let h = show(GROWS, "aaaaaaaaa bbbbbbbbb ccccccccc", 0);
    assert_eq!(height(&h), 3.0 * 20.0);
}

#[test]
fn without_field_sizing_a_textarea_keeps_its_rows() {
    let h = show("min-height:2lh;max-height:5lh", "a\nb\nc\nd", 0);
    assert_eq!(height(&h), 2.0 * 20.0);
}

/// The text's black pixels in rows `from` to `to` of the textarea's column.
fn ink(h: &Host, from: u32, to: u32) -> usize {
    let f = h.frame("x").unwrap();
    (from..to.min(f.height))
        .flat_map(|y| (0..100).map(move |x| (x, y)))
        .map(|(x, y)| &f.rgba[((y * f.width + x) * 4) as usize..][..3])
        .filter(|p| p[0] == p[1] && p[1] == p[2] && p[0] < 128)
        .count()
}

#[test]
fn past_max_height_a_textarea_scrolls_to_its_caret_and_clips() {
    let text = "WWWW\n\n\n\n\n\n";
    let start = show(GROWS, text, 0);
    assert_eq!(height(&start), 100.0);
    assert!(
        ink(&start, 0, 20) > 0,
        "the first line is shown at the start"
    );
    assert_eq!(ink(&start, 100, 140), 0, "nothing past the box");
    let end = show(GROWS, text, text.chars().count());
    assert_eq!(height(&end), 100.0);
    assert_eq!(ink(&end, 0, 20), 0, "the caret at the end scrolled it away");
    assert_eq!(ink(&end, 100, 140), 0, "nothing past the box");
}

/// Growing and shrinking repaint what the box covers: a partial paint is
/// the full one.
#[test]
fn a_textarea_that_grows_or_shrinks_repaints_what_it_covers() {
    let mut h = show(GROWS, "", 0);
    for value in ["a\nb\nc\nd", "a\nb\nc\nd\ne\nf\ng", "a", ""] {
        h.set_text_field("x", "t", value, 0, 0);
        h.render_dirty(&mut |_, _, _| {});
        let partial = h.frame("x").unwrap().rgba.clone();
        h.repaint();
        h.render_dirty(&mut |_, _, _| {});
        let full = &h.frame("x").unwrap().rgba;
        let worst = partial
            .iter()
            .zip(full)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(worst <= 1, "{value:?}: off by {worst}");
    }
}

/// The rows of the last `fit` event the host has to send, if any.
fn fit_rows(h: &mut Host) -> Option<u64> {
    h.render_dirty(&mut |_, _, _| {});
    let mut rows = None;
    for e in h.take_events() {
        if let Effect::Reply(b) = e {
            let mut s = Splitter::with_max(hotty_blitz::MAX_COMMAND_BYTES);
            s.feed(&b, &mut |ev| {
                if let Output::Command(c) = ev
                    && c.get("e") == Some("fit")
                {
                    rows = body(&c.payload)["r"].as_u64();
                }
            });
        }
    }
    rows
}

/// The height reaches `fit` (SPEC 5.2): a surface placed to fit its
/// document asks for the rows its textarea has grown to.
#[test]
fn the_height_of_a_growing_textarea_reaches_fit() {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x"), ("q", "2")],
        &format!(
            "<style>body{{margin:0}}textarea{{display:block;margin:0;padding:0;border:0;\
             width:100px;font:16px monospace;line-height:20px;{GROWS}}}</style>\
             <textarea id=t></textarea>"
        ),
    ));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "30"),
            ("f", "1"),
            ("q", "2"),
        ],
        "",
    ));
    assert_eq!(fit_rows(&mut h), None, "two rows, as placed");
    h.set_text_field("x", "t", "a\nb\nc\nd", 0, 0);
    assert_eq!(fit_rows(&mut h), Some(4));
    h.set_text_field("x", "t", &"a\n".repeat(9), 0, 0);
    assert_eq!(fit_rows(&mut h), Some(5), "no more than max-height");
}

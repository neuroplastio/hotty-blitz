//! An edit hotty-blitz makes in a field itself (a keymap action such as
//! BackSpace, as typing does) damages the field, not the surface (gov R-22:
//! a BackSpace in plx's palette repainted all of it, 3.4 ms at 1.6).

use hotty::Message;
use hotty_blitz::{Config, Damage, Host, Metrics};

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// A palette at 1.6: a field above a list, the keyboard in the field.
fn palette() -> Host {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 16,
            cell_h: 32,
            scale: 1.6,
        },
        ..Config::default()
    });
    let rows: String = (0..12).map(|i| format!("<li>command {i}</li>")).collect();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "p")],
        &format!(
            "<style>body{{margin:0;font:13px sans-serif;background:#222;color:#ddd}}
            input{{width:300px}}li:nth-child(odd){{background:#333}}</style>
            <input id=f value='open settings'><ul>{rows}</ul>"
        ),
    ));
    h.handle(&cmd(
        &[("a", "place"), ("s", "p"), ("c", "40"), ("r", "12")],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    h.handle(&cmd(&[("a", "focus"), ("s", "p"), ("t", "f")], ""));
    h.render_dirty(&mut |_, _, _| {});
    h
}

/// Renders, and returns the pixels the frame's damage covers, or None for
/// a full paint.
fn damaged(h: &mut Host) -> Option<u64> {
    let mut px = Some(0);
    h.render_dirty(&mut |_, _, d| {
        px = match (px, d) {
            (Some(n), Damage::Rects(rs)) => {
                Some(n + rs.iter().map(|r| r.w as u64 * r.h as u64).sum::<u64>())
            }
            _ => None,
        }
    });
    px
}

/// The frame equals a full paint of the same document, to ±1 a channel on
/// anti-aliased edges.
fn assert_matches_full(h: &mut Host, what: &str) {
    let partial = h.frame("p").unwrap().rgba.clone();
    h.repaint();
    h.render_dirty(&mut |_, _, _| {});
    let whole = &h.frame("p").unwrap().rgba;
    let off = partial
        .iter()
        .zip(whole)
        .filter(|(a, b)| a.abs_diff(**b) > 1)
        .count();
    assert_eq!(off, 0, "{what}: {off} channels differ from a full paint");
}

#[test]
fn a_backspace_damages_the_field_as_a_letter_does() {
    let mut h = palette();
    let frame = {
        let f = h.frame("p").unwrap();
        f.width as u64 * f.height as u64
    };
    assert!(h.key_bytes("p", b"x").consumed);
    let letter = damaged(&mut h).expect("a letter damages the field");
    assert_matches_full(&mut h, "a letter");
    let before = h.frame("p").unwrap().rgba.clone();
    // BackSpace as a press, as bytes (DEL), and ctrl+backspace (a word).
    for (what, press) in [
        ("backspace", true),
        ("\x7f", false),
        ("ctrl+backspace", true),
    ] {
        let used = if press {
            h.key_pressed("p", what, "").consumed
        } else {
            h.key_bytes("p", what.as_bytes()).consumed
        };
        assert!(used, "{what:?} is the field's");
        let px = damaged(&mut h).unwrap_or_else(|| panic!("{what:?}: a full paint"));
        assert!(
            px <= 2 * letter && px * 4 < frame,
            "{what:?}: {px} px damaged, a letter {letter}, the frame {frame}"
        );
        assert_matches_full(&mut h, what);
    }
    assert_ne!(
        h.frame("p").unwrap().rgba,
        before,
        "the edits changed the field"
    );
}

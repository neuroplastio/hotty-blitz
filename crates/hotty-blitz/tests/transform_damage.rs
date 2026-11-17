//! Edits and focus changes inside transformed boxes damage their painted positions.

mod common;

use common::delta_cmd;
use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

fn render(h: &mut Host) {
    h.render_dirty(&mut |_, _, _| {});
}

fn matches_full(h: &mut Host, step: &str) {
    let partial = h.frame("x").unwrap().rgba.clone();
    h.repaint();
    render(h);
    let full = &h.frame("x").unwrap().rgba;
    let off = partial
        .iter()
        .zip(full)
        .filter(|(a, b)| a.abs_diff(**b) > 1)
        .count();
    assert_eq!(off, 0, "{step}: {off} channels differ from a full repaint");
}

#[test]
fn edits_and_focus_in_a_translated_dialog_repaint() {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 16,
            cell_h: 32,
            scale: 1.6,
        },
        ..Config::default()
    });
    h.handle(&cmd(&[("a", "doc"), ("s", "x")],
        "<style>body{margin:0;background:#000;color:#fff} input{display:block;width:200px;height:24px;margin:8px} input:focus{outline:2px solid orange}</style><div style='position:fixed;top:50%;left:50%;translate:-50% -50%;background:#123'><input id=a><input id=b><input id=c></div>"));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "40"), ("r", "12")],
        "",
    ));
    render(&mut h);
    h.handle(&cmd(&[("a", "focus"), ("s", "x"), ("t", "a")], ""));
    render(&mut h);
    matches_full(&mut h, "focus");
    assert!(h.key_bytes("x", b"Acme").consumed);
    render(&mut h);
    matches_full(&mut h, "typing");
    assert!(h.key_pressed("x", "tab", "").consumed);
    render(&mut h);
    matches_full(&mut h, "tab");
    let before = h.frame("x").unwrap().rgba.clone();
    h.handle(&delta_cmd(
        &[("s", "x"), ("n", "1")],
        "attr",
        Some("c"),
        Some("value"),
        "ProjectTest",
    ));
    render(&mut h);
    assert!(
        h.frame("x").unwrap().rgba != before,
        "delta changed the field"
    );
    matches_full(&mut h, "value delta");
}

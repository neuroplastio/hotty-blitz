//! Focus as a browser has it, where hotty-ui's agents screen found Blitz
//! apart: `:focus-within` matches the focused element and its ancestors.

mod common;

use common::delta_cmd;
use hotty::Message;
use hotty_blitz::{Config, Host, Key, KeyName, Metrics, Mods};

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

fn render(h: &mut Host) {
    h.render_dirty(&mut |_, _, _| {});
}

fn show(h: &mut Host, html: &str) {
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], html));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            // Large enough that a few rows repaint alone, not the frame.
            ("c", "120"),
            ("r", "40"),
            ("q", "2"),
        ],
        "",
    ));
    render(h);
}

fn focus(h: &mut Host, t: Option<&str>) {
    let mut c = vec![("a", "focus"), ("s", "x"), ("q", "2")];
    if let Some(t) = t {
        c.push(("t", t));
    }
    h.handle(&cmd(&c, ""));
    render(h);
}

fn tab(h: &mut Host, shift: bool) {
    h.key(
        "x",
        &Key {
            name: KeyName::Tab,
            names: Vec::new(),
            mods: Mods {
                shift,
                ..Mods::default()
            },
        },
    );
    render(h);
}

fn color(h: &Host, id: &str) -> String {
    h.computed_style("x", &format!("#{id}"), "color", false)
}

const RED: &str = "rgb(255, 0, 0)";
const WHITE: &str = "rgb(255, 255, 255)";

/// A list whose selected row `:focus-within` marks red while focus is in
/// the list; a button outside it. A row keeps its height when its button
/// goes, so that only what `:focus-within` restyles changes.
const LIST: &str = "<style>body{margin:0;font:20px monospace;line-height:20px;color:#fff}\
     .list:focus-within .sel{color:#f00}#list:focus-within{color:#f00}\
     #r2,#r3{height:20px}</style>\
     <button id=out>out</button>\
     <div id=list class=list><div id=r1 class=sel>one</div><div style='height:400px'></div>\
     <div id=r2><span><button id=b2>go</button></span></div>\
     <div id=r3><button id=b3>go</button></div></div>";

/// The last frame is what a full paint of the same state draws: what
/// `:focus-within` restyled was repainted, off the focus's own chain too.
fn as_a_full_paint(h: &mut Host, step: &str) {
    let partial = h.frame("x").unwrap().rgba.clone();
    h.repaint();
    render(h);
    let differing = partial
        .chunks(4)
        .zip(h.frame("x").unwrap().rgba.chunks(4))
        .filter(|(p, q)| p != q)
        .count();
    assert_eq!(
        differing, 0,
        "{step}: {differing} pixels differ from a full paint"
    );
}

#[test]
fn focus_within_matches_the_focused_element_s_ancestors() {
    let mut h = host();
    show(&mut h, LIST);
    assert_eq!(color(&h, "r1"), WHITE, "nothing focused");

    focus(&mut h, Some("b2"));
    assert_eq!(color(&h, "list"), RED, "the focused button's ancestor");
    assert_eq!(color(&h, "r1"), RED, "a rule on a descendant of it");
    as_a_full_paint(&mut h, "focused");

    focus(&mut h, Some("out"));
    assert_eq!(color(&h, "list"), WHITE, "focus out of the list");
    assert_eq!(color(&h, "r1"), WHITE);
    as_a_full_paint(&mut h, "focus out");

    tab(&mut h, false);
    assert_eq!(color(&h, "r1"), RED, "Tab into the list");
    as_a_full_paint(&mut h, "Tab");

    // A delta removes the focused button: focus leaves the list.
    h.handle(&delta_cmd(
        &[("s", "x"), ("q", "2")],
        "remove",
        Some("b2"),
        None,
        "",
    ));
    render(&mut h);
    assert_eq!(color(&h, "list"), WHITE, "the focused button removed");
    assert_eq!(color(&h, "r1"), WHITE);
    as_a_full_paint(&mut h, "removed");

    // And a delta that removes an ancestor of the focused one.
    focus(&mut h, Some("b3"));
    assert_eq!(color(&h, "r1"), RED);
    h.handle(&delta_cmd(
        &[("s", "x"), ("q", "2")],
        "remove",
        Some("r3"),
        None,
        "",
    ));
    render(&mut h);
    assert_eq!(color(&h, "list"), WHITE, "its row removed");
    assert_eq!(color(&h, "r1"), WHITE);
    as_a_full_paint(&mut h, "its row removed");
}

//! Focus as a browser has it, where hotty-ui's agents screen found Blitz
//! apart: `:focus-within` matches the focused element and its ancestors,
//! and Tab steps over a control that is not rendered (`visibility: hidden`,
//! or `display: none` on it or an ancestor).

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

/// The ids of `ids` drawn red, which `:focus` makes a button.
fn focused(h: &Host, ids: &[&str]) -> Vec<String> {
    ids.iter()
        .filter(|id| color(h, id) == RED)
        .map(|id| id.to_string())
        .collect()
}

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

/// Rows with actions shown on the selected one alone, as hotty-ui's agents
/// screen has them, and a button that isn't displayed.
const ROWS: &str = "<style>body{margin:0;font:20px monospace;line-height:20px;color:#fff}\
     button:focus{color:#f00}.row:not(.sel) .act{visibility:hidden}</style>\
     <button id=a>a</button>\
     <div id=r1 class='row'><span class=act><button id=h1>x</button></span></div>\
     <div id=r2 class='row sel'><span class=act><button id=s1>y</button></span></div>\
     <div class='row'><button id=h2 class=act>x</button></div>\
     <div style='display:none'><button id=n1>n</button></div>\
     <button id=n2 style='display:none'>n</button>\
     <div style='visibility:hidden'><button id=v1 style='visibility:visible'>v</button></div>\
     <button id=z>z</button>";

/// The buttons that have a computed style to read. One not displayed has
/// none: Tab landing on it shows as a step that focuses none of these.
const ALL: [&str; 6] = ["a", "h1", "s1", "h2", "v1", "z"];

#[test]
fn tab_steps_over_a_control_that_is_not_rendered() {
    let mut h = host();
    show(&mut h, ROWS);
    focus(&mut h, Some("a"));
    assert_eq!(focused(&h, &ALL), ["a"]);

    let mut forward = Vec::new();
    for _ in 0..3 {
        tab(&mut h, false);
        forward.extend(focused(&h, &ALL));
    }
    assert_eq!(
        forward,
        ["s1", "v1", "z"],
        "Tab: the shown row's action and a visible child of a hidden box"
    );

    let mut back = Vec::new();
    for _ in 0..3 {
        tab(&mut h, true);
        back.extend(focused(&h, &ALL));
    }
    assert_eq!(back, ["v1", "s1", "a"], "Shift+Tab");

    // Selecting the other row shows its action: Tab reaches it.
    h.handle(&delta_cmd(
        &[("s", "x"), ("q", "2")],
        "attr",
        Some("r2"),
        Some("class"),
        "row",
    ));
    h.handle(&delta_cmd(
        &[("s", "x"), ("q", "2")],
        "attr",
        Some("r1"),
        Some("class"),
        "row sel",
    ));
    tab(&mut h, false);
    assert_eq!(focused(&h, &ALL), ["h1"], "the row selected by a delta");
}

/// `a=focus` without a target focuses the first control rendered.
#[test]
fn the_surface_s_focus_skips_a_hidden_first_control() {
    let mut h = host();
    show(
        &mut h,
        "<style>body{margin:0;font:20px monospace;color:#fff}button:focus{color:#f00}</style>\
         <button id=h style='visibility:hidden'>h</button><button id=b>b</button>",
    );
    focus(&mut h, None);
    assert_eq!(focused(&h, &["h", "b"]), ["b"]);
}

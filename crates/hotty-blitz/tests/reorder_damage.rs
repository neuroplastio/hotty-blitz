//! A morph that only reorders an element's kept children (a list reordered
//! by drag: none added, none removed) damages the list, so the next frame
//! repaints it. It used to touch the parent only when children came or went,
//! and the reordered list stayed as it was until something else damaged it.

mod common;

use common::delta_cmd;
use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

fn doc(order: &[&str]) -> String {
    let rows: String = order
        .iter()
        .map(|n| format!("<li id={n} class={n}>item {n}</li>"))
        .collect();
    format!(
        "<style>body{{margin:0;font:13px sans-serif;color:#fff}}ul{{margin:0;padding:0;list-style:none}}
        li{{height:20px;padding:2px 6px}}.a{{background:#a33}}.b{{background:#3a3}}
        .c{{background:#33a}}.d{{background:#aa3}}</style><ul id=list>{rows}</ul>"
    )
}

fn host(order: &[&str]) -> Host {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 16,
            cell_h: 32,
            scale: 1.6,
        },
        ..Config::default()
    });
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], &doc(order)));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "30"), ("r", "4")],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    h
}

#[test]
fn a_morph_that_only_reorders_children_repaints_them() {
    let mut h = host(&["a", "b", "c", "d"]);
    let before = h.frame("x").unwrap().rgba.clone();
    let reordered = ["c", "a", "d", "b"];
    let new = doc(&reordered);
    let body = new.split_once("</style>").unwrap().1;
    h.handle(&delta_cmd(&[("s", "x")], "morph", Some("list"), None, body));
    let mut frames = 0;
    h.render_dirty(&mut |_, _, _| frames += 1);
    assert!(frames > 0, "a reorder damages the list: no frame came out");
    let got = h.frame("x").unwrap().rgba.clone();
    assert_ne!(got, before, "the frame still shows the old order");
    let mut fresh = host(&reordered);
    fresh.repaint();
    fresh.render_dirty(&mut |_, _, _| {});
    let want = &fresh.frame("x").unwrap().rgba;
    let off = got
        .iter()
        .zip(want)
        .filter(|(a, b)| a.abs_diff(**b) > 1)
        .count();
    assert_eq!(off, 0, "{off} channels differ from a full paint");
}

//! A flex or grid item whose own style changes its size is laid out again
//! by its container, as a static render of the same document draws it.
//! hotty-surf's gallery sets its canvas's width with a `var` op on the
//! canvas itself, a flex item (`width: calc(var(--u-w) * …)`); after a
//! resize, or a width picked in its toolbar, the canvas kept its old width:
//! speculative relayout asked the item the questions its container asked
//! last time, which carry the old width, and kept the container's layout.

mod common;

use common::delta_cmd;
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

fn shown(doc: &str) -> Host {
    let mut h = host();
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], doc));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "60"), ("r", "4")],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    h
}

/// `doc` after each op, painted after each (as a terminal paints them), and
/// how many pixels differ from `want` drawn fresh.
fn differ(doc: &str, ops: &[(&str, &str, Option<&str>, &str)], want: &str) -> usize {
    let mut h = shown(doc);
    for &(op, t, k, v) in ops {
        h.handle(&delta_cmd(&[("s", "x"), ("n", "1")], op, Some(t), k, v));
        h.render_dirty(&mut |_, _, _| {});
    }
    let fresh = shown(want);
    let (a, b) = (h.frame("x").unwrap(), fresh.frame("x").unwrap());
    assert_eq!((a.width, a.height), (b.width, b.height));
    a.rgba
        .chunks(4)
        .zip(b.rgba.chunks(4))
        .filter(|(p, q)| p != q)
        .count()
}

const CSS: &str = "<style>body{margin:0;font:14px sans-serif;--cw:10px}\
    .row{display:flex}.grid{display:grid;grid-template-columns:auto 1fr}\
    .w{flex:0 0 auto;width:calc(var(--w) * var(--cw));background:#345;color:#eee}\
    .s{overflow:auto}.wide{flex-basis:400px}.b{flex:0 0 200px;background:#345}\
    .tail{background:#8c4}</style>";

#[test]
fn a_var_op_resizes_the_flex_item_that_reads_it() {
    let at = |w: &str| {
        format!(
            "{CSS}<div class=row><div id=c class='w s' style='--w:{w}'>text</div><i class=tail>x</i></div>"
        )
    };
    assert_eq!(
        differ(&at("20"), &[("var", "c", Some("w"), "40")], &at("40")),
        0
    );
    assert_eq!(
        differ(&at("40"), &[("var", "c", Some("w"), "20")], &at("20")),
        0
    );
}

#[test]
fn a_style_attr_resizes_a_flex_item() {
    let at = |w: &str| {
        format!(
            "{CSS}<div class=row><div id=c style='flex:0 0 auto;width:{w}px;background:#345'>text</div><i class=tail>x</i></div>"
        )
    };
    let to = "flex:0 0 auto;width:400px;background:#345";
    assert_eq!(
        differ(&at("200"), &[("attr", "c", Some("style"), to)], &at("400")),
        0
    );
}

#[test]
fn a_class_resizes_a_flex_item_by_its_basis() {
    let at = |class: &str| {
        format!("{CSS}<div class=row><div id=c class='{class}'>text</div><i class=tail>x</i></div>")
    };
    assert_eq!(
        differ(
            &at("b"),
            &[("attr", "c", Some("class"), "b wide")],
            &at("b wide")
        ),
        0
    );
}

#[test]
fn a_var_op_resizes_a_grid_item() {
    let at = |w: &str| {
        format!(
            "{CSS}<div class=grid><div id=c class=w style='--w:{w}'>text</div><i class=tail>x</i></div>"
        )
    };
    assert_eq!(
        differ(&at("20"), &[("var", "c", Some("w"), "40")], &at("40")),
        0
    );
}

#[test]
fn a_text_change_in_a_flex_item_still_draws_right() {
    // Content, not style: the speculation may keep the container, and the
    // result must still be what a fresh render draws.
    let at = |t: &str| {
        format!("{CSS}<div class=row><span id=c class=b>{t}</span><i class=tail>x</i></div>")
    };
    assert_eq!(
        differ(&at("one"), &[("text", "c", None, "three")], &at("three")),
        0
    );
}

/// Damage-only painting after these deltas draws what one full paint of the
/// same document does.
#[test]
fn partial_repaint_matches_a_full_paint() {
    let doc = format!(
        "{CSS}<div class=row><div id=c class='w s' style='--w:20'>text</div><i class=tail>x</i></div>\
         <div class=row><div id=d style='flex:0 0 auto;width:100px;background:#345'>d</div><i class=tail>y</i></div>"
    );
    let ops: [(&str, &str, Option<&str>, &str); 3] = [
        ("var", "c", Some("w"), "40"),
        (
            "attr",
            "d",
            Some("style"),
            "flex:0 0 auto;width:300px;background:#345",
        ),
        ("var", "c", Some("w"), "30"),
    ];
    let mut inc = shown(&doc);
    for &(op, t, k, v) in &ops {
        inc.handle(&delta_cmd(&[("s", "x"), ("n", "1")], op, Some(t), k, v));
        inc.render_dirty(&mut |_, _, _| {});
    }
    let mut full = host();
    full.handle(&cmd(&[("a", "doc"), ("s", "x")], &doc));
    full.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "60"), ("r", "4")],
        "",
    ));
    for &(op, t, k, v) in &ops {
        full.handle(&delta_cmd(&[("s", "x"), ("n", "1")], op, Some(t), k, v));
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
}

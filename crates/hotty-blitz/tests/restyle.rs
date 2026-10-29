//! A class an `attr` delta adds to an ancestor restyles its descendants:
//! text that inherits `color` takes the new colour, as a static render of
//! the same document draws it. hotty-ui's agents screen marks its list
//! stale by adding `u-tone-muted` to it, and a live run was seen to keep
//! every name in its old colour. These pin what it sends (the ops a stale
//! list makes, and the same stream replayed and run live, are drawn
//! right): a regression, should one come.

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

fn render(h: &mut Host) {
    h.render_dirty(&mut |_, _, _| {});
}

fn show(h: &mut Host, html: &str) {
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], html));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "40"),
            ("r", "4"),
            ("q", "2"),
        ],
        "",
    ));
    render(h);
}

fn delta(h: &mut Host, op: &str, t: &str, k: Option<&str>, v: &str) {
    h.handle(&delta_cmd(&[("s", "x"), ("q", "2")], op, Some(t), k, v));
    render(h);
}

/// One delta of several ops, each `(op, t, k, v)`.
fn ops(h: &mut Host, ops: &[(&str, &str, Option<&str>, &str)]) {
    let ops: Vec<serde_json::Value> = ops
        .iter()
        .map(|&(op, t, k, v)| {
            let mut o = serde_json::json!({ "op": op, "t": t, "v": v });
            if let Some(k) = k {
                o["k"] = k.into();
            }
            o
        })
        .collect();
    let body = rmp_serde::to_vec_named(&serde_json::json!({ "ops": ops })).unwrap();
    h.handle(&Message::new(
        [("a", "delta"), ("s", "x"), ("q", "2")]
            .into_iter()
            .collect(),
        body,
    ));
    render(h);
}

/// The surface's last frame, as RGBA.
fn frame(h: &Host) -> Vec<u8> {
    h.frame("x").unwrap().rgba.clone()
}

/// Whether some pixel of the frame is about `c`, as text's middle is.
fn draws(h: &Host, c: [u8; 3]) -> bool {
    frame(h)
        .chunks(4)
        .any(|p| p[0].abs_diff(c[0]) <= 2 && p[1].abs_diff(c[1]) <= 2 && p[2].abs_diff(c[2]) <= 2)
}

const STYLE: &str = "<style>body{margin:0;font:20px monospace;line-height:20px}\
     .s{--fg:#ffffff;--bg:#000000;--muted:color-mix(in srgb, var(--fg) 50%, var(--bg));\
     color:var(--fg);background:var(--bg);min-height:100vh}\
     .row{display:flex;flex-direction:row;align-items:center;min-width:0}\
     .t{white-space:pre;overflow:hidden;text-overflow:ellipsis;min-width:0}\
     .b{font-weight:700}.muted{color:var(--muted)}</style>";

/// A list in a surface, white on black, one of whose rows `muted` marks
/// grey; `#list` gets the class.
fn list(class: &str) -> String {
    format!(
        "{STYLE}<div id=r class=s><div id=list class='{class}'>\
         <div id=a class=row><span id=ag class='t'>W</span><span id=a0 class='t b'>WWWW</span></div>\
         <div id=b class=row><span id=b0 class='t'>WWWW</span></div>\
         </div></div>"
    )
}

/// The colour `#id` is drawn in, as getComputedStyle gives it.
fn color(h: &Host, id: &str) -> String {
    h.computed_style("x", &format!("#{id}"), "color", false)
}

#[test]
fn a_class_added_to_an_ancestor_restyles_inheriting_text() {
    // As a static render has it.
    let mut want = host();
    show(&mut want, &list("muted"));
    let grey = color(&want, "a0");
    assert_eq!(grey, color(&want, "list"));

    let mut h = host();
    show(&mut h, &list(""));
    assert_eq!(color(&h, "a0"), "rgb(255, 255, 255)");
    assert!(draws(&h, [255, 255, 255]));

    delta(&mut h, "attr", "list", Some("class"), "muted");
    assert_eq!(color(&h, "list"), grey, "the list");
    assert_eq!(color(&h, "a"), grey, "a row");
    assert_eq!(color(&h, "a0"), grey, "a row's text");
    assert_eq!(color(&h, "b0"), grey, "another row's text");
    assert!(
        !draws(&h, [255, 255, 255]),
        "no text is drawn white any more"
    );
    assert!(
        frame(&h) == frame(&want),
        "drawn as the same document is from scratch"
    );

    delta(&mut h, "attr", "list", Some("class"), "");
    assert_eq!(color(&h, "a0"), "rgb(255, 255, 255)", "the class gone");
    assert!(draws(&h, [255, 255, 255]));
}

/// hotty-ui's agents screen going stale: one delta inserts a banner, adds
/// the class to the list, and adds it to a glyph in each row as well.
#[test]
fn a_class_added_to_an_ancestor_and_a_descendant_in_one_delta() {
    let mut want = host();
    show(&mut want, &list("muted"));
    let grey = color(&want, "a0");

    for (name, delta) in [
        (
            "the list, then a glyph",
            vec![
                ("attr", "list", Some("class"), "muted"),
                ("attr", "ag", Some("class"), "t muted"),
            ],
        ),
        (
            "a glyph, then the list",
            vec![
                ("attr", "ag", Some("class"), "t muted"),
                ("attr", "list", Some("class"), "muted"),
            ],
        ),
        (
            "an insert, then the list",
            vec![
                (
                    "before",
                    "list",
                    None,
                    "<div id=banner class=muted>banner</div>",
                ),
                ("attr", "list", Some("class"), "muted"),
            ],
        ),
    ] {
        let mut h = host();
        show(&mut h, &list(""));
        ops(&mut h, &delta);
        assert_eq!(color(&h, "list"), grey, "{name}: the list");
        assert_eq!(color(&h, "ag"), grey, "{name}: the glyph");
        assert_eq!(color(&h, "a0"), grey, "{name}: a row's text");
        assert_eq!(color(&h, "b0"), grey, "{name}: another row's text");
        assert!(!draws(&h, [255, 255, 255]), "{name}: no text drawn white");
    }
}

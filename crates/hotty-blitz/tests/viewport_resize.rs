//! A surface resized while it holds an element no style traversal has
//! reached: one a delta inserted into a `display: none` subtree, or one
//! inserted in the frame of the resize. A document that uses viewport units
//! has every element that uses them restyled when the viewport changes, and
//! the walk that finds them asked each element for its style, so it
//! panicked on one that had none ("called `Option::unwrap()` on a `None`
//! value" in Stylo's `ElementStyles::primary`), and the surface was lost
//! (mods/stylo 0002).

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

fn place(h: &mut Host, rows: &str) {
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "40"), ("r", rows)],
        "",
    ));
}

fn append(h: &mut Host, t: &str, html: &str) {
    h.handle(&delta_cmd(&[("s", "x")], "append", Some(t), None, html));
}

/// The resize took: the frame is the new size, and the list's `100vh` is
/// the new viewport's height.
fn resized(h: &Host) {
    assert_eq!(h.frame("x").map(|f| f.height), Some(240));
    assert_eq!(h.computed_style("x", "#list", "min-height", false), "240px");
}

/// A list that fills the viewport's height, and a hidden one beside it.
const DOC: &str = "<body style='margin:0'><ul id=list style='min-height:100vh'><li id=a>a</li></ul>\
                   <div id=hidden style='display:none'><p id=b>b</p></div></body>";

#[test]
fn a_resize_passes_over_an_element_inserted_under_display_none() {
    let mut h = host();
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], DOC));
    place(&mut h, "10");
    render(&mut h);
    // Never styled: its parent is display:none. A render later, a resize
    // still finds it.
    append(&mut h, "hidden", "<p id=c>c</p>");
    render(&mut h);
    place(&mut h, "12");
    render(&mut h);
    resized(&h);
}

#[test]
fn a_resize_passes_over_rows_inserted_in_its_frame() {
    let mut h = host();
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], DOC));
    place(&mut h, "10");
    render(&mut h);
    append(&mut h, "list", "<li id=r1>one</li><li id=r2>two</li>");
    place(&mut h, "12");
    render(&mut h);
    resized(&h);
}

//! A document that replaces another on its surface, with the same `cid:`
//! sheets, takes the author style data the old one built, rather than
//! building it again. On its own in this binary: the count it reads is
//! the process's.

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn handle(h: &mut Host, pairs: &[(&str, &str)], payload: &[u8]) {
    h.handle(&Message::new(
        pairs.iter().copied().collect(),
        payload.to_vec(),
    ));
}

fn pixel(h: &Host, s: &str, x: u32, y: u32) -> [u8; 3] {
    let f = h.frame(s).unwrap();
    let i = ((y * f.width + x) * 4) as usize;
    [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
}

#[test]
fn a_document_replaced_with_the_same_sheets_reuses_their_style_data() {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    handle(
        &mut h,
        &[("a", "res"), ("id", "theme"), ("type", "text/css")],
        b"html{background:#f00} p.b{color:#00f}",
    );
    let page = |text: &str| {
        format!("<link rel=stylesheet href='cid:theme'><p class=b>{text}</p>").into_bytes()
    };
    handle(&mut h, &[("a", "doc"), ("s", "x")], &page("one"));
    handle(
        &mut h,
        &[("a", "place"), ("s", "x"), ("c", "10"), ("r", "2")],
        b"",
    );
    h.render_dirty(&mut |_, _, _| {});
    let before = Host::author_data_reused();
    // A tab switch: the one surface gets another document with the sheet.
    handle(&mut h, &[("a", "doc"), ("s", "x")], &page("two"));
    h.render_dirty(&mut |_, _, _| {});
    assert_eq!(pixel(&h, "x", 99, 39), [255, 0, 0]);
    assert_eq!(
        Host::author_data_reused(),
        before + 1,
        "the replaced document's author data is taken"
    );
}

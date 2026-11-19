//! SPEC §7.1: `cid:` in an SVG `href` is resolved for `<use>` too. A
//! `<use href="cid:sprite#id">` draws element `id` of SVG resource
//! `sprite`, as `<use href="#id">` draws one of the document's: in the
//! context of the `<use>`, `currentColor` included. Sent after the
//! document or sent again, it is drawn again (mods/blitz 0104).

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

fn cmd(h: &mut Host, pairs: &[(&str, &str)], payload: &str) {
    h.handle(&Message::new(
        pairs.iter().copied().collect(),
        payload.as_bytes().to_vec(),
    ));
}

fn res(h: &mut Host, id: &str, svg: &str) {
    cmd(
        h,
        &[("a", "res"), ("id", id), ("type", "image/svg+xml")],
        svg,
    );
}

fn draw(h: &mut Host) {
    h.render_dirty(&mut |_, _, _| {});
}

fn pixel(h: &Host, x: u32, y: u32) -> [u8; 3] {
    let f = h.frame("x").unwrap();
    let i = ((y * f.width + x) * 4) as usize;
    [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
}

const BLACK: [u8; 3] = [0, 0, 0];
const RED: [u8; 3] = [255, 0, 0];
const LIME: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];

/// A sprite: a square in `currentColor`, one that refers to it, and one
/// filled with a gradient of the sprite's own.
fn sprite(square_fill: &str) -> String {
    format!(
        "<svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink'>\
         <defs><linearGradient id='g'><stop offset='0' stop-color='#00f'/>\
         <stop offset='1' stop-color='#00f'/></linearGradient></defs>\
         <symbol id='square' viewBox='0 0 20 20'><rect width='20' height='20' fill='{square_fill}'/></symbol>\
         <symbol id='alias' viewBox='0 0 20 20'><use xlink:href='#square' width='20' height='20'/></symbol>\
         <symbol id='grad' viewBox='0 0 20 20'><rect width='20' height='20' fill='url(#g)'/></symbol>\
         </svg>"
    )
}

/// One 20×20 `<svg>` per row, each with a `<use>` of `href` in `color`.
fn uses(rows: &[(&str, &str)]) -> String {
    let mut html = String::from(
        "<body style='margin:0;background:#000'>\
         <svg style='display:none'><symbol id='square' viewBox='0 0 20 20'>\
         <rect width='20' height='20' fill='#ff0'/></symbol></svg>",
    );
    for (href, color) in rows {
        html.push_str(&format!(
            "<svg width=20 height=20 style='display:block;color:{color}'>\
             <use href='{href}' width=20 height=20/></svg>"
        ));
    }
    html + "</body>"
}

fn show(h: &mut Host, html: &str) {
    cmd(h, &[("a", "doc"), ("s", "x")], html);
    cmd(h, &[("a", "place"), ("s", "x"), ("c", "2"), ("r", "4")], "");
    draw(h);
}

#[test]
fn a_use_draws_an_element_of_an_svg_resource() {
    let mut h = host();
    res(&mut h, "sprite", &sprite("currentColor"));
    show(
        &mut h,
        &uses(&[
            ("cid:sprite#square", "red"),
            ("cid:sprite#alias", "lime"),
            ("cid:sprite#grad", "red"),
            ("cid:sprite#none", "red"),
        ]),
    );
    // currentColor is the <use>'s, and the document's own #square, yellow,
    // is not the sprite's.
    assert_eq!(pixel(&h, 10, 10), RED);
    assert_eq!(pixel(&h, 10, 30), LIME, "through a <use> in the sprite");
    assert_eq!(pixel(&h, 10, 50), BLUE, "the sprite's own gradient");
    assert_eq!(pixel(&h, 10, 70), BLACK, "no such element");
}

#[test]
fn a_use_draws_its_resource_when_it_arrives_and_when_it_is_sent_again() {
    let mut h = host();
    show(
        &mut h,
        "<body style='margin:0;background:#000'>\
         <svg width=20 height=20 style='display:block;color:red'>\
         <use xlink:href='cid:sprite#square' width=20 height=20/></svg></body>",
    );
    assert_eq!(pixel(&h, 10, 10), BLACK, "not sent yet");
    res(&mut h, "sprite", &sprite("currentColor"));
    draw(&mut h);
    assert_eq!(pixel(&h, 10, 10), RED, "sent after the document");
    res(&mut h, "sprite", &sprite("#00f"));
    draw(&mut h);
    assert_eq!(pixel(&h, 10, 10), BLUE, "sent again");
}

#[test]
fn a_use_whose_href_changes_draws_the_new_element() {
    let mut h = host();
    res(&mut h, "sprite", &sprite("currentColor"));
    show(
        &mut h,
        "<body style='margin:0;background:#000'>\
         <svg width=20 height=20 style='display:block;color:red'>\
         <use id=u href='#nothing' width=20 height=20/></svg></body>",
    );
    assert_eq!(pixel(&h, 10, 10), BLACK);
    let body = rmp_serde::to_vec_named(&serde_json::json!({
        "ops": [{"op": "attr", "t": "u", "k": "href", "v": "cid:sprite#grad"}]
    }))
    .unwrap();
    h.handle(&Message::new(
        [("a", "delta"), ("s", "x")].into_iter().collect(),
        body,
    ));
    draw(&mut h);
    assert_eq!(pixel(&h, 10, 10), BLUE);
}

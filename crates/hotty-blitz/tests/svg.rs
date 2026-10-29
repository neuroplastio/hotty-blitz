//! Inline SVG follows its colour: `fill="currentColor"` (on a shape, or on
//! the `svg` for its shapes to inherit) takes the colour the element has
//! now, after a delta, a theme change, hover or focus, as a browser draws
//! it. Icons drawn this way change colour with no delta at all (`:hover`,
//! `:focus`), so a program cannot work around a stale colour.

mod common;

use common::delta_cmd;
use hotty::Message;
use hotty_blitz::{Config, Host, Metrics, Mods, PointerKind};

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

/// The pixel at `(x, y)` of the surface's last frame, as RGB.
fn pixel(h: &Host, s: &str, x: u32, y: u32) -> [u8; 3] {
    let f = h.frame(s).unwrap();
    let i = ((y * f.width + x) * 4) as usize;
    [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
}

fn delta(h: &mut Host, op: &str, t: &str, k: Option<&str>, payload: &str) {
    h.handle(&delta_cmd(
        &[("s", "x"), ("q", "2")],
        op,
        Some(t),
        k,
        payload,
    ));
    render(h);
}

const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const YELLOW: [u8; 3] = [255, 255, 0];
const MAGENTA: [u8; 3] = [255, 0, 255];

/// A 40px square icon at the surface's top left, in `.w`, whose colour is
/// the theme's `--c`; `.b` makes it blue, hover magenta, and the button's
/// focus yellow. `svg` is the icon's markup.
fn place(h: &mut Host, svg: &str) {
    let html = format!(
        "<style id=theme>:root{{--c:#f00}}</style>\
         <style>body{{margin:0}}button{{all:unset;display:block}}\
         .w{{color:var(--c);width:40px;height:40px}}.b{{color:#00f}}\
         #h:hover{{color:#f0f}}#k:focus{{color:#ff0}}</style>\
         <div id=w class=w>{svg}</div>\
         <div id=h class=w>{svg}</div>\
         <button id=k class=w>{svg}</button>"
    );
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], &html));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "10"),
            ("r", "6"),
            ("q", "2"),
        ],
        "",
    ));
    render(h);
}

/// The icon in `#w`, `#h` and `#k`: their centres, 40px apart down.
const W: (u32, u32) = (20, 20);
const H: (u32, u32) = (20, 60);
const K: (u32, u32) = (20, 100);

fn follows(svg: &str) {
    let mut h = host();
    place(&mut h, svg);
    assert_eq!(
        pixel(&h, "x", W.0, W.1),
        RED,
        "first drawn: the theme's colour"
    );

    delta(&mut h, "attr", "w", Some("class"), "w b");
    assert_eq!(pixel(&h, "x", W.0, W.1), BLUE, "a class on its parent");

    delta(&mut h, "text", "theme", None, ":root{--c:#0f0}");
    assert_eq!(pixel(&h, "x", H.0, H.1), GREEN, "a theme's style text");
    assert_eq!(pixel(&h, "x", W.0, W.1), BLUE, "the class still wins");

    delta(&mut h, "attr", "w", Some("class"), "w");
    assert_eq!(
        pixel(&h, "x", W.0, W.1),
        GREEN,
        "the class gone, the theme's"
    );

    h.pointer(
        "x",
        PointerKind::Move,
        H.0 as f32,
        H.1 as f32,
        Mods::default(),
    );
    render(&mut h);
    assert_eq!(pixel(&h, "x", H.0, H.1), MAGENTA, "hover, with no delta");
    h.pointer("x", PointerKind::Move, 90.0, 20.0, Mods::default());
    render(&mut h);
    assert_eq!(pixel(&h, "x", H.0, H.1), GREEN, "hover gone");

    h.handle(&cmd(
        &[("a", "focus"), ("s", "x"), ("t", "k"), ("q", "2")],
        "",
    ));
    render(&mut h);
    assert_eq!(pixel(&h, "x", K.0, K.1), YELLOW, "focus, with no delta");
}

#[test]
fn a_shape_filled_with_current_color_follows_its_colour() {
    follows(
        r#"<svg width="40" height="40" viewBox="0 0 40 40"><rect width="40" height="40" fill="currentColor"/></svg>"#,
    );
}

#[test]
fn shapes_that_inherit_current_color_from_the_svg_follow_its_colour() {
    follows(
        r#"<svg width="40" height="40" viewBox="0 0 40 40" fill="currentColor"><path d="M0 0H40V40H0Z"/></svg>"#,
    );
}

#[test]
fn an_attribute_changed_inside_an_svg_is_drawn() {
    // The same staleness, without colour: a delta to a shape's attribute.
    let mut h = host();
    place(
        &mut h,
        r##"<svg width="40" height="40" viewBox="0 0 40 40"><rect id=r width="40" height="40" fill="#f00"/></svg>"##,
    );
    assert_eq!(pixel(&h, "x", W.0, W.1), RED);
    // Ids repeat across the three icons: the delta reaches the first.
    delta(&mut h, "attr", "r", Some("fill"), "#00f");
    assert_eq!(pixel(&h, "x", W.0, W.1), BLUE);
}

#[test]
fn current_color_from_a_color_mix_is_drawn() {
    // color-mix() computes to an sRGB colour that serializes as
    // color(srgb …), as oklab and display-p3 colours do. usvg reads only
    // rgb(), and drew the icon black: every icon in a kit's muted text.
    let mut h = host();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x"), ("q", "2")],
        "<style>body{margin:0}div{width:40px;height:40px;\
         color:color-mix(in srgb, #f00 50%, #00f)}</style>\
         <div><svg width=\"40\" height=\"40\" viewBox=\"0 0 40 40\">\
         <rect width=\"40\" height=\"40\" fill=\"currentColor\"/></svg></div>",
    ));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "10"),
            ("r", "2"),
            ("q", "2"),
        ],
        "",
    ));
    render(&mut h);
    let [r, g, b] = pixel(&h, "x", 20, 20);
    assert!(
        r.abs_diff(128) <= 1 && g == 0 && b.abs_diff(128) <= 1,
        "half red, half blue: {:?}",
        [r, g, b]
    );
}

/// A 40px square `svg` at the surface's top left, black behind it, styled
/// by `css` (in the document's stylesheet, with `--c` red), with `shapes`
/// in a 40-unit viewBox.
fn icon(css: &str, shapes: &str) -> Host {
    let mut h = host();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x"), ("q", "2")],
        &format!(
            "<style id=theme>:root{{--c:#f00}}</style>\
             <style>body{{margin:0;background:#000}}svg{{display:block}}{css}</style>\
             <svg class=icon width=\"40\" height=\"40\" viewBox=\"0 0 40 40\">{shapes}</svg>"
        ),
    ));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "10"),
            ("r", "4"),
            ("q", "2"),
        ],
        "",
    ));
    render(&mut h);
    h
}

#[test]
fn a_document_rule_fills_a_shape() {
    let h = icon(".icon rect{fill:#00f}", r#"<rect width="40" height="40"/>"#);
    assert_eq!(pixel(&h, "x", 20, 20), BLUE);
}

#[test]
fn a_document_rule_with_a_custom_property_fills_a_shape() {
    let mut h = icon(
        ".icon rect{fill:var(--c)}",
        r#"<rect width="40" height="40"/>"#,
    );
    assert_eq!(pixel(&h, "x", 20, 20), RED);
    delta(&mut h, "text", "theme", None, ":root{--c:#0f0}");
    assert_eq!(pixel(&h, "x", 20, 20), GREEN, "the theme changed");
}

#[test]
fn a_custom_property_in_a_shape_s_style_fills_it() {
    let h = icon(
        "",
        r#"<rect width="40" height="40" style="fill:var(--c)"/>"#,
    );
    assert_eq!(pixel(&h, "x", 20, 20), RED);
}

#[test]
fn a_document_rule_strokes_a_shape() {
    // A 4-unit stroke along x = 20, in the theme's colour; nothing filled.
    let h = icon(
        ".icon path{stroke:var(--c);stroke-width:4px;fill:none}",
        r#"<path d="M20 0V40"/>"#,
    );
    assert_eq!(pixel(&h, "x", 20, 20), RED);
    assert_eq!(pixel(&h, "x", 10, 20), [0, 0, 0]);
}

#[test]
fn a_document_rule_beats_a_presentation_attribute() {
    // As in a browser: presentation attributes are the cascade's lowest
    // author-level hints.
    let h = icon(
        ".icon rect{fill:#0f0}",
        r##"<rect width="40" height="40" fill="#f00"/>"##,
    );
    assert_eq!(pixel(&h, "x", 20, 20), GREEN);
}

#[test]
fn presentation_attributes_still_draw() {
    let h = icon(
        "",
        r##"<g fill="#00f"><rect width="20" height="40"/><rect x="20" width="20" height="40" fill="#0f0"/></g>"##,
    );
    assert_eq!(pixel(&h, "x", 10, 20), BLUE, "inherited from the group");
    assert_eq!(pixel(&h, "x", 30, 20), GREEN, "its own");
}

#[test]
fn a_used_symbol_inherits_from_the_use() {
    let h = icon(
        "",
        r##"<defs><symbol id="s" viewBox="0 0 40 40"><rect width="40" height="40"/></symbol></defs><use href="#s" width="40" height="40" fill="#0f0"/>"##,
    );
    assert_eq!(pixel(&h, "x", 20, 20), GREEN);
}

#[test]
fn the_svg_s_own_opacity_applies_once() {
    // Blitz paints it on the svg's box; usvg is not to apply it again.
    let mut h = host();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x"), ("q", "2")],
        "<style>body{margin:0;background:#000}svg{display:block}</style>\
         <svg opacity=\"0.5\" width=\"40\" height=\"40\" viewBox=\"0 0 40 40\">\
         <rect width=\"40\" height=\"40\" fill=\"#f00\"/></svg>",
    ));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "10"),
            ("r", "4"),
            ("q", "2"),
        ],
        "",
    ));
    render(&mut h);
    let [r, g, b] = pixel(&h, "x", 20, 20);
    assert!(
        r.abs_diff(128) <= 2 && g == 0 && b == 0,
        "half red: {:?}",
        [r, g, b]
    );
}

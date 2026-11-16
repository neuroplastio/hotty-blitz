//! An SVG colour in CSS Color 4's `oklch()` or `oklab()` paints, in an
//! inline SVG and in one a `data:` URL brings as a background image.
//! Tailwind 4 writes its palette in oklch; basecoat draws a picked select
//! option's check as `background-image: var(--check-icon)`, a data: SVG
//! with `stroke="oklch(0.556 0 0)"`, and svgtypes took that for an
//! invalid paint: Chromium drew the check, hotty-blitz an empty box.

mod common;

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

fn shown(html: &str) -> Host {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], html));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "20"),
            ("r", "4"),
            ("q", "2"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    h.render_dirty(&mut |_, _, _| {});
    h
}

fn pixel(h: &Host, x: u32, y: u32) -> [u8; 3] {
    let f = h.frame("x").unwrap();
    let i = ((y * f.width + x) * 4) as usize;
    [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
}

fn near(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 1)
}

/// A 40px square filled with `fill`, as an inline SVG and as a data: SVG
/// background through a custom property.
fn squares(fill: &str) -> String {
    let svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='24' height='24'>\
         <rect width='24' height='24' fill='{fill}'/></svg>"
    );
    format!(
        "<style>body{{margin:0;background:#000}}div{{width:40px;height:40px;line-height:0}}\
         #b{{--i:url(\"data:image/svg+xml;utf8,{svg}\");background:var(--i) no-repeat 0 0/100%}}\
         </style><div>{svg}</div><div id=b></div>"
    )
}

#[test]
fn oklch_and_oklab_fills_paint() {
    // Tailwind's neutral-500 (#737373) and red-500 (#fb2c36).
    for (fill, rgb) in [
        ("oklch(0.556 0 0)", [115, 115, 115]),
        ("oklch(55.6% 0 none)", [115, 115, 115]),
        ("oklab(0.556 0 0)", [115, 115, 115]),
        ("oklch(0.637 0.237 25.331)", [251, 44, 54]),
    ] {
        let h = shown(&squares(fill));
        assert!(
            near(pixel(&h, 12, 12), rgb),
            "{fill} inline: {:?}",
            pixel(&h, 12, 12)
        );
        assert!(
            near(pixel(&h, 12, 52), rgb),
            "{fill} background: {:?}",
            pixel(&h, 12, 52)
        );
    }
}

/// basecoat's check: a stroked path in a data: SVG through a custom
/// property, sized and placed by the background shorthand.
#[test]
fn basecoats_check_icon_is_drawn() {
    let html = r#"<style>body{margin:0;background:#000}
.opt{--check-icon:url("data:image/svg+xml;utf8,<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"oklch(0.556 0 0)\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"M20 6 9 17l-5-5\"/></svg>");width:160px;height:32px;background:#333 var(--check-icon) no-repeat right .5rem center/.875rem}
</style><div class=opt></div>"#;
    let h = shown(html);
    // The check's 14px box: right 8px, centred in the 32px row.
    let inked = (138..152)
        .flat_map(|x| (9..23).map(move |y| (x, y)))
        .filter(|&(x, y)| pixel(&h, x, y) != [0x33, 0x33, 0x33])
        .count();
    assert!(inked > 10, "the check drew {inked} pixels");
    assert_eq!(pixel(&h, 100, 16), [0x33, 0x33, 0x33]);
}

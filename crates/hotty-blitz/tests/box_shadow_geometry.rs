//! Inset isolation, scaled offsets and per-corner shadow geometry.

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn render(style: &str, scale: f32) -> (u32, Vec<u8>) {
    let html = format!(
        "<style>html,body{{margin:0;background:#000}}.box{{position:absolute;left:20px;top:20px;width:40px;height:40px;{style}}}</style><div class=box></div>"
    );
    let mut host = Host::new(Config {
        metrics: Metrics {
            cell_w: (10.0 * scale) as u32,
            cell_h: (20.0 * scale) as u32,
            scale,
        },
        ..Config::default()
    });
    let cmd = |pairs: &[(&str, &str)], text: &str| {
        Message::new(pairs.iter().copied().collect(), text.as_bytes().to_vec())
    };
    host.handle(&cmd(&[("a", "doc"), ("s", "x")], &html));
    host.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "12"), ("r", "6")],
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    let f = host.frame("x").unwrap();
    (f.width, f.rgba.clone())
}

fn rgb(width: u32, pixels: &[u8], x: u32, y: u32) -> [u8; 3] {
    let i = ((y * width + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2]]
}

#[test]
fn blurred_inset_shadow_does_not_erase_background() {
    let (w, p) = render(
        "background:#f00;box-shadow:inset 0 0 8px #000,inset 0 0 12px #000",
        1.0,
    );
    assert!(
        rgb(w, &p, 40, 40)[0] > 240,
        "the box center must retain its red background"
    );
}

#[test]
fn inset_shadow_offset_scales_with_device_pixels() {
    let (w, p) = render("background:#fff;box-shadow:inset 8px 0 0 #f00", 2.0);
    assert!(
        rgb(w, &p, 52, 80)[1] < 32,
        "the 16-device-pixel band should cover x=52"
    );
    assert!(
        rgb(w, &p, 60, 80)[1] > 220,
        "the interior past the band stays white"
    );
}

#[test]
fn outset_shadow_keeps_a_square_corner() {
    let (w, p) = render(
        "background:#000;border-radius:20px 0 0 0;box-shadow:0 0 0 8px #f00",
        1.0,
    );
    assert!(
        rgb(w, &p, 67, 12)[0] > 220,
        "the square top-right corner should cast a square shadow"
    );
}

#[test]
fn a_blurred_outset_shadow_keeps_a_square_corner() {
    let (w, p) = render(
        "background:#000;border-radius:20px 0 0 0;box-shadow:0 0 8px 8px #f00",
        1.0,
    );
    let red = rgb(w, &p, 64, 14)[0];
    assert!(
        (145..=165).contains(&red),
        "the square top-right corner retains its blur (red={red})"
    );
}

//! A resource sent again with the type and bytes it has changes nothing
//! (SPEC §7.1: every surface that refers to it would draw what it draws):
//! no parse, no restyle, no frame, no event. One byte or the type changed,
//! and it is a resource sent again.

use hotty::Message;
use hotty::host::{Output, Splitter};
use hotty_blitz::{Config, Effect, Host, Metrics};

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

fn handle(h: &mut Host, pairs: &[(&str, &str)], payload: &[u8]) -> Vec<Message> {
    let effects = h.handle(&Message::new(
        pairs.iter().copied().collect(),
        payload.to_vec(),
    ));
    replies(&effects)
}

fn replies(effects: &[Effect]) -> Vec<Message> {
    let mut out = Vec::new();
    let mut s = Splitter::with_max(hotty_blitz::MAX_COMMAND_BYTES);
    for e in effects {
        if let Effect::Reply(b) = e {
            s.feed(b, &mut |ev| {
                if let Output::Command(c) = ev {
                    out.push(c);
                }
            });
        }
    }
    out
}

fn res(h: &mut Host, id: &str, mime: &str, bytes: &[u8]) {
    let r = handle(
        h,
        &[("a", "res"), ("id", id), ("type", mime), ("n", "1")],
        bytes,
    );
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].get("a"), Some("ok"));
}

/// Draws what is dirty: how many frames were drawn.
fn draw(h: &mut Host) -> usize {
    let mut n = 0;
    h.render_dirty(&mut |_, _, _| n += 1);
    n
}

fn pixel(h: &Host, s: &str, x: u32, y: u32) -> [u8; 3] {
    let f = h.frame(s).unwrap();
    let i = ((y * f.width + x) * 4) as usize;
    [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
}

fn png(w: u32, h: u32, [r, g, b]: [u8; 3]) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([r, g, b, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

const SHEET: &[u8] = b"html{background:#f00}";
const RED: [u8; 3] = [255, 0, 0];

/// Surface `x`, fitted to an image 60 px tall (`f=1`), and `y`, both
/// linking `theme`; drawn, with the fit heard.
fn two_surfaces(h: &mut Host) {
    res(h, "theme", "text/css", SHEET);
    res(h, "pic", "image/png", &png(20, 60, [0, 0, 255]));
    let html = "<body style='margin:0'><img src='cid:pic' style='display:block'>\
                <link rel=stylesheet href='cid:theme'></body>";
    handle(h, &[("a", "doc"), ("s", "x")], html.as_bytes());
    handle(
        h,
        &[("a", "place"), ("s", "x"), ("c", "10"), ("f", "1")],
        b"",
    );
    handle(h, &[("a", "doc"), ("s", "y")], html.as_bytes());
    handle(
        h,
        &[("a", "place"), ("s", "y"), ("c", "10"), ("r", "4")],
        b"",
    );
    draw(h);
    h.take_events();
    assert_eq!(pixel(h, "y", 99, 79), RED);
}

#[test]
fn an_identical_resend_changes_nothing() {
    let mut h = host();
    two_surfaces(&mut h);
    let parsed = h.sheets_parsed();
    let frames = (
        h.frame("x").unwrap().rgba.clone(),
        h.frame("y").unwrap().rgba.clone(),
    );
    res(&mut h, "theme", "text/css", SHEET);
    res(&mut h, "pic", "image/png", &png(20, 60, [0, 0, 255]));
    // (`has_dirty` is true after any command: the menu is checked again.)
    assert_eq!(draw(&mut h), 0, "no surface restyles or draws again");
    assert_eq!(h.sheets_parsed(), parsed, "no parse");
    assert!(h.take_events().is_empty(), "no fit, nothing");
    assert!(h.frame("x").unwrap().rgba == frames.0);
    assert!(h.frame("y").unwrap().rgba == frames.1);
}

#[test]
fn a_resend_with_one_byte_changed_updates_every_surface() {
    let mut h = host();
    two_surfaces(&mut h);
    let parsed = h.sheets_parsed();
    res(&mut h, "theme", "text/css", b"html{background:#f0f}");
    assert_eq!(draw(&mut h), 2);
    assert_eq!(pixel(&h, "x", 99, 59), [255, 0, 255]);
    assert_eq!(pixel(&h, "y", 99, 79), [255, 0, 255]);
    assert_eq!(h.sheets_parsed(), parsed + 1);
    // An image one byte longer, which draws the same: it is loaded again,
    // and the surfaces that show it are drawn again.
    let mut pic = png(20, 60, [0, 0, 255]);
    pic.push(0);
    res(&mut h, "pic", "image/png", &pic);
    assert_eq!(draw(&mut h), 2);
}

#[test]
fn the_same_bytes_as_another_type_are_a_resource_sent_again() {
    let mut h = host();
    two_surfaces(&mut h);
    let parsed = h.sheets_parsed();
    res(&mut h, "theme", "text/plain", SHEET);
    assert_eq!(draw(&mut h), 2);
    assert_eq!(h.sheets_parsed(), parsed + 1);
}

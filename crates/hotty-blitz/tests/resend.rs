//! A resource sent again with the type and bytes it has changes nothing,
//! in every case: no parse, no restyle, no frame, no event, and an
//! animation plays on where it was. One byte or the type changed, and it
//! replaces the resource.

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

#[test]
fn a_resource_deleted_and_sent_again_reaches_its_surfaces() {
    let mut h = host();
    two_surfaces(&mut h);
    let parsed = h.sheets_parsed();
    handle(&mut h, &[("a", "del"), ("id", "theme")], b"");
    // The same bytes as before it was deleted: a sending like any first.
    res(&mut h, "theme", "text/css", SHEET);
    assert_eq!(draw(&mut h), 2);
    assert_eq!(h.sheets_parsed(), parsed + 1);
    assert_eq!(pixel(&h, "y", 99, 79), RED);
}

#[test]
fn a_document_waiting_for_a_resource_gets_it() {
    let mut h = host();
    res(&mut h, "theme", "text/css", SHEET);
    handle(&mut h, &[("a", "del"), ("id", "theme")], b"");
    // Linked while it is missing: the document waits for it.
    handle(
        &mut h,
        &[("a", "doc"), ("s", "x")],
        b"<body style='margin:0'><p>x</p><link rel=stylesheet href='cid:theme'></body>",
    );
    handle(
        &mut h,
        &[("a", "place"), ("s", "x"), ("c", "10"), ("r", "2")],
        b"",
    );
    draw(&mut h);
    assert_ne!(pixel(&h, "x", 99, 39), RED);
    res(&mut h, "theme", "text/css", SHEET);
    assert_eq!(draw(&mut h), 1);
    assert_eq!(pixel(&h, "x", 99, 39), RED);
    // And sent once more as it is: nothing to do.
    res(&mut h, "theme", "text/css", SHEET);
    assert_eq!(draw(&mut h), 0);
}

/// A GIF that shows `first`, then green, each for a long while.
fn gif(first: [u8; 3]) -> Vec<u8> {
    use image::codecs::gif::{GifEncoder, Repeat};
    let mut out = Vec::new();
    {
        let mut enc = GifEncoder::new(&mut out);
        enc.set_repeat(Repeat::Infinite).unwrap();
        let frames = [first, [0, 255, 0]].map(|[r, g, b]| {
            let img = image::RgbaImage::from_pixel(20, 20, image::Rgba([r, g, b, 255]));
            image::Frame::from_parts(img, 0, 0, image::Delay::from_numer_denom_ms(5000, 1))
        });
        enc.encode_frames(frames).unwrap();
    }
    out
}

#[test]
fn an_animation_sent_again_as_it_is_plays_on_where_it_was() {
    let mut h = host();
    res(&mut h, "anim", "image/gif", &gif(RED));
    handle(
        &mut h,
        &[("a", "doc"), ("s", "x")],
        b"<body style='margin:0'><img src='cid:anim' style='display:block'></body>",
    );
    handle(
        &mut h,
        &[("a", "place"), ("s", "x"), ("c", "4"), ("r", "2")],
        b"",
    );
    draw(&mut h);
    assert_eq!(pixel(&h, "x", 10, 10), RED);
    let due = h.next_frame().expect("it plays");
    h.animate(due);
    draw(&mut h);
    assert_eq!(pixel(&h, "x", 10, 10), [0, 255, 0]);
    let next = h.next_frame();
    res(&mut h, "anim", "image/gif", &gif(RED));
    assert_eq!(draw(&mut h), 0, "nothing loaded again");
    assert_eq!(pixel(&h, "x", 10, 10), [0, 255, 0], "on its second frame");
    assert_eq!(h.next_frame(), next, "on the same clock");
}

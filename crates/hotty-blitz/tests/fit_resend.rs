//! SPEC §5.2, §7.1: a resource replaced (other bytes or another type) is
//! drawn again, and a placement with `f=1` hears when that changes the rows
//! its document needs. Sent again as it is, it changes nothing.

mod common;

use common::body;
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
    replies(&h.handle(&Message::new(
        pairs.iter().copied().collect(),
        payload.to_vec(),
    )))
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

fn png(w: u32, h: u32, [r, g, b]: [u8; 3]) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([r, g, b, 255]));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn pixel(h: &Host, s: &str, x: u32, y: u32) -> [u8; 3] {
    let f = h.frame(s).unwrap();
    let i = ((y * f.width + x) * 4) as usize;
    [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
}

/// Sends `pic`, draws, and returns the `fit` events.
fn send(h: &mut Host, pic: &[u8]) -> Vec<Message> {
    handle(
        h,
        &[("a", "res"), ("id", "pic"), ("type", "image/png")],
        pic,
    );
    h.render_dirty(&mut |_, _, _| {});
    replies(&h.take_events())
        .into_iter()
        .filter(|e| e.get("e") == Some("fit"))
        .collect()
}

fn fits_of(events: &[Message]) -> Vec<serde_json::Value> {
    events.iter().map(|e| body(&e.payload)).collect()
}

#[test]
fn an_image_sent_again_taller_is_heard_with_fit() {
    let mut h = host();
    send(&mut h, &png(20, 60, [0, 0, 255]));
    handle(
        &mut h,
        &[("a", "doc"), ("s", "x")],
        b"<body style='margin:0;background:#000'><img src='cid:pic' style='display:block'></body>",
    );
    let r = handle(
        &mut h,
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "10"),
            ("f", "1"),
            ("n", "1"),
        ],
        b"",
    );
    assert_eq!(r[0].get("r"), Some("3"));
    h.render_dirty(&mut |_, _, _| {});
    h.take_events();
    assert_eq!(pixel(&h, "x", 10, 10), [0, 0, 255]);

    let fits = send(&mut h, &png(20, 80, [0, 255, 0]));
    assert_eq!(
        pixel(&h, "x", 10, 10),
        [0, 255, 0],
        "the new image is drawn"
    );
    assert_eq!(fits_of(&fits), vec![serde_json::json!({ "r": 4 })]);
    // Back to 60 px: three rows again.
    let fits = send(&mut h, &png(20, 60, [255, 0, 0]));
    assert_eq!(pixel(&h, "x", 10, 10), [255, 0, 0]);
    assert_eq!(fits_of(&fits), vec![serde_json::json!({ "r": 3 })]);
}

#[test]
fn a_background_image_sent_again_taller_is_heard_with_fit() {
    // The same resource as a background, which sizes nothing, and as an
    // <img>, which does: both are drawn again, and the rows are heard.
    let mut h = host();
    send(&mut h, &png(20, 60, [0, 0, 255]));
    handle(
        &mut h,
        &[("a", "doc"), ("s", "x")],
        b"<body style='margin:0;background:#000'>\
          <div style='height:20px;background:url(cid:pic)'></div>\
          <img src='cid:pic' style='display:block'></body>",
    );
    handle(
        &mut h,
        &[("a", "place"), ("s", "x"), ("c", "10"), ("f", "1")],
        b"",
    );
    h.render_dirty(&mut |_, _, _| {});
    h.take_events();
    assert_eq!(pixel(&h, "x", 5, 5), [0, 0, 255]);
    let fits = send(&mut h, &png(20, 100, [0, 255, 0]));
    assert_eq!(fits_of(&fits), vec![serde_json::json!({ "r": 6 })]);
    assert_eq!(pixel(&h, "x", 5, 5), [0, 255, 0], "the background too");
}

/// A GIF that shows `first`, then green, each for a long while.
fn gif([r, g, b]: [u8; 3]) -> Vec<u8> {
    use image::codecs::gif::{GifEncoder, Repeat};
    let mut out = Vec::new();
    {
        let mut enc = GifEncoder::new(&mut out);
        enc.set_repeat(Repeat::Infinite).unwrap();
        let frames = [[r, g, b], [0, 255, 0]].map(|[r, g, b]| {
            let img = image::RgbaImage::from_pixel(20, 20, image::Rgba([r, g, b, 255]));
            image::Frame::from_parts(img, 0, 0, image::Delay::from_numer_denom_ms(5000, 1))
        });
        enc.encode_frames(frames).unwrap();
    }
    out
}

/// Sends GIF `bytes` as `anim`, draws: how many frames were drawn.
fn send_gif(h: &mut Host, bytes: &[u8]) -> usize {
    handle(
        h,
        &[("a", "res"), ("id", "anim"), ("type", "image/gif")],
        bytes,
    );
    let mut n = 0;
    h.render_dirty(&mut |_, _, _| n += 1);
    n
}

#[test]
fn a_playing_image_is_reloaded_only_when_it_is_replaced() {
    let mut h = host();
    send_gif(&mut h, &gif([255, 0, 0]));
    handle(
        &mut h,
        &[("a", "doc"), ("s", "x")],
        b"<body style='margin:0'><img src='cid:anim' style='display:block'></body>",
    );
    handle(
        &mut h,
        &[("a", "place"), ("s", "x"), ("c", "4"), ("f", "1")],
        b"",
    );
    h.render_dirty(&mut |_, _, _| {});
    let due = h.next_frame().expect("it plays");
    h.animate(due);
    h.render_dirty(&mut |_, _, _| {});
    assert_eq!(pixel(&h, "x", 10, 10), [0, 255, 0]);
    // The same bytes: not fetched again, it plays on.
    assert_eq!(send_gif(&mut h, &gif([255, 0, 0])), 0);
    assert_eq!(pixel(&h, "x", 10, 10), [0, 255, 0], "not started over");
    // Other bytes replace it: fetched again, from its first frame.
    assert_eq!(send_gif(&mut h, &gif([0, 0, 255])), 1);
    assert_eq!(pixel(&h, "x", 10, 10), [0, 0, 255]);
}

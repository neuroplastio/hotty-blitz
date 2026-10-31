//! Inset box-shadows without blur: the ring a hairline is drawn with
//! (mods/blitz/inset-ring.md).

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// One box at the top left of a black document, black itself, with the
/// given inset shadow in white, at 1.6: the frame's luma, row by row.
fn paint(shadow: &str) -> (usize, Vec<u8>) {
    let html = format!(
        "<html><style>html,body{{margin:0;background:#000}}
        .b{{width:100px;height:40px;border-radius:12px;background:#000;box-shadow:{shadow}}}
        </style><div class=b></div></html>"
    );
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 16,
            cell_h: 32,
            scale: 1.6,
        },
        ..Config::default()
    });
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], &html));
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
    let f = h.frame("x").unwrap();
    (f.width as usize, f.rgba.chunks(4).map(|p| p[1]).collect())
}

/// `inset 0 0 0 1px` at 1.6 is a ring 1.6 device pixels wide, drawn as two
/// whole pixels on each straight edge (the hole's edge on a device pixel,
/// where Blitz's unblurred hole put it), and nothing inside: the box is
/// 160×64 device pixels.
#[test]
fn a_hairline_inset_shadow_is_two_whole_pixels_at_1_6() {
    let (w, px) = paint("inset 0 0 0 1px #fff");
    let at = |x: usize, y: usize| px[y * w + x];
    for (x, y) in [
        (80, 0),
        (80, 1),
        (80, 62),
        (80, 63),
        (0, 32),
        (1, 32),
        (158, 32),
        (159, 32),
    ] {
        assert_eq!(at(x, y), 255, "ring at {x},{y}");
    }
    for (x, y) in [(80, 2), (80, 61), (2, 32), (157, 32), (80, 32)] {
        assert_eq!(at(x, y), 0, "inside at {x},{y}");
    }
}

/// A spread past half the box leaves no hole: the whole padding box is
/// shadow, as CSS says. Blitz cut a hole out of the middle, the inverted
/// rectangle made positive.
#[test]
fn an_inset_spread_past_half_the_box_shadows_all_of_it() {
    let (w, px) = paint("inset 0 0 0 30px #fff");
    let at = |x: usize, y: usize| px[y * w + x];
    for (x, y) in [(80, 32), (40, 32), (120, 32), (80, 10), (80, 54)] {
        assert_eq!(at(x, y), 255, "shadow at {x},{y}");
    }
}

/// A spread of zero or less leaves the hole over the whole padding box: no
/// shadow at all. Blitz's layers left the rounded corners' anti-aliasing
/// behind, up to 38 of 255 at 1.6.
#[test]
fn an_inset_spread_of_zero_or_less_paints_nothing() {
    for shadow in ["inset 0 0 0 0 #fff", "inset 0 0 0 -2px #fff"] {
        let (_, px) = paint(shadow);
        let lit = px.iter().filter(|&&v| v > 0).count();
        assert_eq!(lit, 0, "{shadow}: {lit} lit of {}", px.len());
    }
}

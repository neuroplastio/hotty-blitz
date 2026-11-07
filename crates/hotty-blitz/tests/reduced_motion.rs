//! `prefers-reduced-motion` (SPEC §8) follows the user's setting on the
//! system the terminal runs on: a document's `@media` rule matches while
//! `Host::set_reduced_motion(true)` is in force, the document is drawn again
//! when it changes, and `no-preference` is what a host starts with.
use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

const DOC: &str = "<style>\
    html, body { margin: 0; } \
    #a { width: 100px; height: 40px; background: rgb(200, 0, 0); } \
    #b { width: 100px; height: 40px; background: rgb(0, 0, 200); } \
    @media (prefers-reduced-motion: reduce) { #a { background: rgb(0, 200, 0); } } \
    @media (prefers-reduced-motion: no-preference) { #b { background: rgb(200, 200, 0); } } \
    </style><div id=a></div><div id=b></div>";

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

fn add(h: &mut Host, name: &str) {
    h.handle(&cmd(&[("a", "doc"), ("s", name), ("q", "2")], DOC));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", name),
            ("c", "10"),
            ("r", "4"),
            ("q", "2"),
        ],
        "",
    ));
}

/// The pixels at (5, 5), in #a, and (5, 50), in #b, of `name`'s frame.
fn draw(h: &mut Host, name: &str) -> [[u8; 3]; 2] {
    h.render_dirty(&mut |_, _, _| {});
    let f = h.frame(name).unwrap();
    let px = |x: u32, y: u32| {
        let i = ((y * f.width + x) * 4) as usize;
        [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
    };
    [px(5, 5), px(5, 50)]
}

const RED: [u8; 3] = [200, 0, 0];
const GREEN: [u8; 3] = [0, 200, 0];
const BLUE: [u8; 3] = [0, 0, 200];
const YELLOW: [u8; 3] = [200, 200, 0];

#[test]
fn a_host_starts_with_no_preference() {
    let mut h = host();
    add(&mut h, "x");
    assert_eq!(draw(&mut h, "x"), [RED, YELLOW]);
}

#[test]
fn the_rule_matches_only_while_the_setting_is_on() {
    let mut h = host();
    h.set_reduced_motion(true);
    add(&mut h, "x");
    assert_eq!(draw(&mut h, "x"), [GREEN, BLUE]);
}

#[test]
fn turning_it_on_and_off_changes_the_frame_both_ways_without_a_reload() {
    let mut h = host();
    add(&mut h, "x");
    assert_eq!(draw(&mut h, "x"), [RED, YELLOW]);
    h.set_reduced_motion(true);
    assert_eq!(draw(&mut h, "x"), [GREEN, BLUE]);
    h.set_reduced_motion(false);
    assert_eq!(draw(&mut h, "x"), [RED, YELLOW]);
}

#[test]
fn every_surface_is_drawn_again_when_it_changes() {
    let mut h = host();
    add(&mut h, "x");
    add(&mut h, "y");
    draw(&mut h, "x");
    let mut drawn = Vec::new();
    h.set_reduced_motion(true);
    h.render_dirty(&mut |name, _, _| drawn.push(name.to_string()));
    drawn.sort();
    assert_eq!(drawn, ["x", "y"]);
    // The same setting again changes nothing.
    h.set_reduced_motion(true);
    let mut again = 0;
    h.render_dirty(&mut |_, _, _| again += 1);
    assert_eq!(again, 0);
}

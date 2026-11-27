//! A surface nobody can see (`Host::set_seen`): scrolled out of the
//! terminal's view, or in a hidden window. What plays on it asks for no
//! frames, and once it is seen again it asks at once.

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};
use std::time::Duration;

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

fn render(h: &mut Host) -> usize {
    let mut drawn = 0;
    h.render_dirty(&mut |_, _, _| drawn += 1);
    drawn
}

const PULSE: &str = "<style>@keyframes p{50%{opacity:.2}}</style>\
<body style='margin:0'><div style='width:40px;height:20px;background:#6c6;\
animation:p 1s linear infinite'></div></body>";

fn placed(h: &mut Host, name: &str, html: &str) {
    h.handle(&cmd(&[("a", "doc"), ("s", name)], html));
    h.handle(&cmd(
        &[("a", "place"), ("s", name), ("c", "8"), ("r", "2")],
        "",
    ));
    render(h);
}

#[test]
fn an_unseen_surface_asks_for_no_frames() {
    let mut h = host();
    placed(&mut h, "x", PULSE);
    assert!(h.next_frame().is_some(), "the animation plays while seen");
    h.set_seen("x", false);
    assert_eq!(h.next_frame(), None, "nothing plays where nobody sees it");
    std::thread::sleep(Duration::from_millis(40));
    assert!(!h.has_dirty(), "its animation leaves nothing to render");
    assert_eq!(render(&mut h), 0, "and no frame goes out");
}

#[test]
fn seen_again_it_asks_for_a_frame_at_once() {
    let mut h = host();
    placed(&mut h, "x", PULSE);
    h.set_seen("x", false);
    std::thread::sleep(Duration::from_millis(40));
    h.set_seen("x", true);
    let due = h.next_frame().expect("the animation plays again");
    assert!(
        due <= std::time::Instant::now(),
        "the frame it missed is due now"
    );
    assert!(h.has_dirty());
    assert_eq!(render(&mut h), 1, "a frame at the time it is now");
}

#[test]
fn only_the_unseen_surface_stops() {
    let mut h = host();
    placed(&mut h, "a", PULSE);
    placed(&mut h, "b", PULSE);
    h.set_seen("a", false);
    assert!(h.next_frame().is_some(), "the surface still seen plays");
    let future = h.next_frame().unwrap() + Duration::from_millis(1);
    h.animate(future);
    let mut names = Vec::new();
    h.render_dirty(&mut |n, _, _| names.push(n.to_string()));
    assert_eq!(names, ["b"], "only the seen surface is drawn");
}

#[test]
fn an_unseen_surface_still_renders_what_a_command_changes() {
    let mut h = host();
    placed(&mut h, "x", PULSE);
    h.set_seen("x", false);
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], "<p>new</p>"));
    assert!(h.has_dirty());
    assert_eq!(render(&mut h), 1, "a command is not something that plays");
}

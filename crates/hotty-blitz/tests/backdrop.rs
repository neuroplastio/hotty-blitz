//! Backdrops (hotty 0.6, V6-06): a placement with `b=1` that the terminal
//! draws under the cells. It reaches the terminal as one only where the
//! terminal said it draws them, takes no pointer and never has the
//! keyboard: `a=focus` on one is EINVAL, a focused surface placed as one
//! gives the keyboard back, and what a terminal passes it anyway does
//! nothing.

mod common;

use common::body;
use hotty::Message;
use hotty::host::{Output, Splitter};
use hotty_blitz::{Config, Effect, Host, Metrics, Mods, PointerKind, Touch, TouchPhase};

fn host(backdrops: bool) -> Host {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    h.set_backdrops(backdrops);
    h
}

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
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

/// The kinds of the events in `effects`, `e` then `t`.
fn events(effects: &[Effect]) -> Vec<(String, String)> {
    replies(effects)
        .iter()
        .filter(|c| c.get("a") == Some("ev"))
        .map(|c| {
            (
                c.get("e").unwrap_or("").to_string(),
                c.get("t").unwrap_or("").to_string(),
            )
        })
        .collect()
}

/// A canvas with a button, the size of a 20×4 placement.
const DOC: &str = "<style>body{margin:0} button{width:200px;height:80px}</style>\
    <button id=go>go</button>";

/// Places `x` with `extra`: whether the terminal is given it as a
/// backdrop, and the other effects.
fn place(h: &mut Host, extra: &[(&str, &str)]) -> (Option<bool>, Vec<Effect>) {
    let mut pairs = vec![
        ("a", "place"),
        ("s", "x"),
        ("c", "20"),
        ("r", "4"),
        ("q", "2"),
    ];
    pairs.extend_from_slice(extra);
    let fx = h.handle(&cmd(&pairs, ""));
    let backdrop = fx.iter().find_map(|e| match e {
        Effect::Place { backdrop, .. } => Some(*backdrop),
        _ => None,
    });
    h.render_dirty(&mut |_, _, _| {});
    (backdrop, fx)
}

fn show(h: &mut Host, extra: &[(&str, &str)]) -> Option<bool> {
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], DOC));
    place(h, extra).0
}

fn click(h: &mut Host) -> Vec<Effect> {
    let mut fx = h.pointer("x", PointerKind::Down, 50.0, 30.0, Mods::default());
    fx.extend(h.pointer("x", PointerKind::Up, 50.0, 30.0, Mods::default()));
    fx
}

#[test]
fn b_is_the_placement_s_and_only_where_the_terminal_draws_backdrops() {
    let mut h = host(true);
    assert_eq!(show(&mut h, &[("b", "1")]), Some(true));
    // The placement's, as `z` is: placed again without it, the surface is
    // above the cells again, and `b=0` is not a backdrop.
    assert_eq!(place(&mut h, &[]).0, Some(false));
    assert_eq!(place(&mut h, &[("b", "1")]).0, Some(true));
    assert_eq!(place(&mut h, &[("b", "0")]).0, Some(false));
    // A terminal that draws none never gets one: `b` is a key its host
    // does not know (SPEC §15), and the surface is an ordinary placement.
    let mut h = host(false);
    assert_eq!(show(&mut h, &[("b", "1")]), Some(false));
    let r = replies(&h.handle(&cmd(&[("a", "focus"), ("s", "x"), ("t", "go")], "")));
    assert!(r.iter().all(|m| m.get("a") != Some("err")), "{r:?}");
    assert!(h.is_focused("x"));
}

#[test]
fn a_backdrop_never_has_the_keyboard() {
    let mut h = host(true);
    show(&mut h, &[("b", "1")]);
    let r = replies(&h.handle(&cmd(
        &[("a", "focus"), ("s", "x"), ("t", "go"), ("n", "3")],
        "",
    )));
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!((r[0].get("a"), r[0].get("n")), (Some("err"), Some("3")));
    assert_eq!(body(&r[0].payload)["code"], "EINVAL");
    assert!(!h.is_focused("x"));
    // A focused surface placed as a backdrop gives the keyboard back.
    place(&mut h, &[]);
    h.handle(&cmd(&[("a", "focus"), ("s", "x"), ("t", "go")], ""));
    assert!(h.is_focused("x"));
    let (backdrop, fx) = place(&mut h, &[("b", "1")]);
    assert_eq!(backdrop, Some(true));
    assert_eq!(events(&fx), vec![("blur".to_string(), String::new())]);
    assert!(!h.is_focused("x"));
    assert_eq!(h.focused_surface(), None);
}

#[test]
fn a_backdrop_takes_no_pointer() {
    let mut h = host(true);
    // It asked for presses and hovers: a backdrop sees no pointer, so it
    // hears neither, and a click neither clicks nor focuses.
    show(&mut h, &[("b", "1"), ("p", "1"), ("v", "1")]);
    assert!(!h.takes_pointer("x", 50.0, 30.0));
    let mut fx = h.pointer("x", PointerKind::Move, 40.0, 30.0, Mods::default());
    fx.extend(click(&mut h));
    fx.extend(h.pointer("x", PointerKind::Leave, 40.0, 30.0, Mods::default()));
    assert_eq!(events(&fx), vec![]);
    assert!(!h.is_focused("x"));
    // Nor a wheel, nor a touch: they are the terminal's.
    let w = h.wheel("x", 50.0, 30.0, 0.0, 40.0, Mods::default());
    assert!(!w.taken && w.effects.is_empty());
    let t = h.touch("x", TouchPhase::Down, 50.0, 30.0, Mods::default());
    assert_eq!(t.touch, Touch::Terminal);
    let t = h.touch("x", TouchPhase::Up, 50.0, 30.0, Mods::default());
    assert_eq!(t.touch, Touch::Terminal);
    assert_eq!(events(&t.effects), vec![]);

    // Placed above the cells again, the same click is the document's.
    place(&mut h, &[]);
    assert!(h.takes_pointer("x", 50.0, 30.0));
    let ev = events(&click(&mut h));
    assert!(
        ev.contains(&("press".to_string(), "go".to_string())),
        "{ev:?}"
    );
    assert!(
        ev.contains(&("click".to_string(), "go".to_string())),
        "{ev:?}"
    );
    assert!(h.is_focused("x"));
}

#[test]
fn a_gesture_held_before_the_surface_became_a_backdrop_ends() {
    let mut h = host(true);
    show(&mut h, &[("v", "1")]);
    // Over the button, then pressed on it, then placed as a backdrop with
    // the button held: the release still ends the press, and the pointer's
    // leaving says it left.
    let mut fx = h.pointer("x", PointerKind::Move, 50.0, 30.0, Mods::default());
    fx.extend(h.pointer("x", PointerKind::Down, 50.0, 30.0, Mods::default()));
    assert!(events(&fx).contains(&("hover".to_string(), "go".to_string())));
    place(&mut h, &[("b", "1"), ("v", "1")]);
    let mut fx = h.pointer("x", PointerKind::Up, 50.0, 30.0, Mods::default());
    fx.extend(h.pointer("x", PointerKind::Leave, 250.0, 30.0, Mods::default()));
    let ev = replies(&fx);
    let hover = ev
        .iter()
        .filter(|m| m.get("e") == Some("hover"))
        .last()
        .expect("a hover");
    assert_eq!(body(&hover.payload)["out"], true, "{ev:?}");
    // Then nothing more.
    let fx = h.pointer("x", PointerKind::Move, 50.0, 30.0, Mods::default());
    assert_eq!(events(&fx), vec![]);
}

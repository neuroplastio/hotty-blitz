//! Hit reads a surface's own drag target without moving its pointer.
mod common;

use common::{body, delta_cmd};
use hotty::Message;
use hotty::host::{Output, Splitter};
use hotty_blitz::{Config, Effect, Host, Metrics, Mods, PointerKind};

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

fn replies(effects: &[Effect]) -> Vec<Message> {
    let mut out = Vec::new();
    let mut splitter = Splitter::with_max(hotty_blitz::MAX_COMMAND_BYTES);
    for effect in effects {
        if let Effect::Reply(bytes) = effect {
            splitter.feed(bytes, &mut |output| {
                if let Output::Command(message) = output {
                    out.push(message);
                }
            });
        }
    }
    out
}

#[test]
fn hit_uses_the_scrolled_document_without_changing_hover_or_focus() {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 20,
            cell_h: 40,
            scale: 2.0,
        },
        ..Config::default()
    });
    let html = "<style>body{margin:0}div{height:40px}</style><div id=a data-on='drag'>a</div><div id=b data-on='drop' data-steps='2 2'>b</div><div id=c>c</div>";
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("scroll", "1")], html));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "10"),
            ("r", "2"),
            ("v", "1"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    h.take_events();
    h.pointer("x", PointerKind::Move, 10.0, 10.0, Mods::default());
    assert_eq!(h.hit("x", 0, 0).unwrap().t, "a");
    // Query immediately after a scroll, before the next render. The cell is
    // in CSS pixels despite the device scale, and steps match a drag's.
    h.handle(&delta_cmd(
        &[("s", "x")],
        "scroll",
        None,
        None,
        "scroll-down",
    ));
    let hit = h.hit("x", 0, 0).unwrap();
    assert_eq!((hit.t.as_str(), hit.tx, hit.ty), ("b", Some(0), Some(1)));
    assert_eq!(h.focused_surface(), None);
    // Querying the second row does not move the stored pointer's hover.
    let out = h.handle(&cmd(
        &[
            ("a", "hit"),
            ("s", "x"),
            ("c", "0"),
            ("r", "1"),
            ("n", "42"),
        ],
        "",
    ));
    let out = replies(&out);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].get("n"), Some("42"));
    assert_eq!(
        body(&out[0].payload),
        serde_json::json!({"t":"b", "tx":0, "ty":2})
    );
    let left = replies(&h.pointer("x", PointerKind::Leave, 10.0, 10.0, Mods::default()));
    assert_eq!(left.len(), 1);
    assert_eq!(body(&left[0].payload), serde_json::json!({"out":true}));
    assert!(h.hit("x", i64::MAX, i64::MIN).unwrap().t.is_empty());
}

#[test]
fn a_hit_can_read_a_backdrop_and_empty_or_detached_surfaces() {
    let mut h = Host::new(Config::default());
    h.set_backdrops(true);
    for s in ["under", "over"] {
        h.handle(&cmd(
            &[("a", "doc"), ("s", s)],
            "<style>body{margin:0}</style><div id=d data-on=drop style='height:100px'>d</div>",
        ));
        h.handle(&cmd(
            &[
                ("a", "place"),
                ("s", s),
                ("c", "10"),
                ("r", "2"),
                ("b", if s == "under" { "1" } else { "0" }),
            ],
            "",
        ));
    }
    assert_eq!(h.hit("under", 0, 0).unwrap().t, "d");
    h.handle(&cmd(&[("a", "hide"), ("s", "under")], ""));
    assert!(h.hit("under", 0, 0).unwrap().t.is_empty());
    h.handle(&cmd(&[("a", "detach"), ("s", "over")], ""));
    assert_eq!(h.hit("over", 0, 0).unwrap_err().code, "EDETACHED");
    assert_eq!(h.hit("missing", 0, 0).unwrap_err().code, "ENOENT");
}

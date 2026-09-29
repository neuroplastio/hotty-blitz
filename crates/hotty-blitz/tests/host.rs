//! Host behaviour, end to end through commands: what a program can rely on.

use hotty_blitz::{Config, Effect, Host, Key, KeyName, Metrics, Mods, PointerKind};
use hotty_wire::{Command, Event, Scanner};

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

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Command {
    Command::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// Decodes every HOTTY message in the effects' replies.
fn replies(effects: &[Effect]) -> Vec<Command> {
    let mut out = Vec::new();
    let mut s = Scanner::new();
    for e in effects {
        if let Effect::Reply(b) = e {
            s.feed(b, &mut |ev| {
                if let Event::Command(c) = ev {
                    out.push(c);
                }
            });
        }
    }
    out
}

fn render(h: &mut Host) -> Vec<(String, u32, u32, hotty_blitz::Damage)> {
    let mut frames = Vec::new();
    h.render_dirty(&mut |name, f, damage| {
        frames.push((name.to_string(), f.width, f.height, damage.clone()))
    });
    frames
}

#[test]
fn query_answers_with_capabilities() {
    let mut h = host();
    let r = replies(&h.handle(&cmd(&[("a", "q"), ("n", "7")], "")));
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].get("a"), Some("ok"));
    assert_eq!(r[0].get("n"), Some("7"));
    let caps: serde_json::Value = serde_json::from_slice(&r[0].payload).unwrap();
    assert_eq!(caps["cell"]["h"], 20);
    assert!(caps["ops"].as_array().unwrap().iter().any(|o| o == "morph"));
}

#[test]
fn auto_rows_fit_the_content_and_frames_are_cell_sized() {
    let mut h = host();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x")],
        "<div style='height: 95px'>tall</div>",
    ));
    let fx = h.handle(&cmd(&[("a", "place"), ("s", "x"), ("c", "30")], ""));
    let place = fx.iter().find_map(|e| match e {
        Effect::Place { cols, rows, .. } => Some((*cols, *rows)),
        _ => None,
    });
    // 95 px of content in 20 px rows needs 5 rows.
    assert_eq!(place, Some((30, 5)));
    let frames = render(&mut h);
    assert_eq!(
        frames,
        vec![("x".to_string(), 300, 100, hotty_blitz::Damage::Full)]
    );
    // Nothing changed: nothing to send.
    assert!(render(&mut h).is_empty());
}

#[test]
fn placing_again_moves_without_a_frame_until_one_is_asked_for() {
    let mut h = host();
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], "<p>still</p>"));
    let place = cmd(&[("a", "place"), ("s", "x"), ("c", "30"), ("r", "2")], "");
    h.handle(&place);
    assert_eq!(render(&mut h).len(), 1);
    // A move, or another window: the adapter already has the pixels.
    h.handle(&place);
    assert!(render(&mut h).is_empty());
    // An adapter that lost them (its terminal erased the image) asks again.
    h.redeliver("x");
    assert_eq!(
        render(&mut h),
        vec![("x".to_string(), 300, 40, hotty_blitz::Damage::Full)]
    );
    assert!(render(&mut h).is_empty());
}

#[test]
fn a_window_is_part_of_the_surface() {
    let mut h = host();
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], "<p>tall</p>"));
    let place = |h: &mut Host, extra: &[(&str, &str)]| {
        let mut pairs = vec![("a", "place"), ("s", "x"), ("c", "30"), ("r", "8")];
        pairs.extend_from_slice(extra);
        h.handle(&cmd(&pairs, "")).into_iter().find_map(|e| match e {
            Effect::Place { cols, rows, window, .. } => Some((cols, rows, window)),
            _ => None,
        })
    };
    let win = |x, y, w, h| hotty_blitz::Window { x, y, w, h };
    // The whole surface by default; a window to the edges from x and y.
    assert_eq!(place(&mut h, &[]), Some((30, 8, win(0, 0, 30, 8))));
    assert_eq!(place(&mut h, &[("y", "3")]), Some((30, 8, win(0, 3, 30, 5))));
    assert_eq!(place(&mut h, &[("y", "2"), ("h", "4"), ("x", "5"), ("w", "10")]), Some((30, 8, win(5, 2, 10, 4))));
    // The document keeps its size: the frame is the whole surface.
    let frames = render(&mut h);
    assert_eq!((frames[0].1, frames[0].2), (300, 160));
    // Outside the surface, or no cells: EINVAL, and nothing is placed.
    assert_eq!(place(&mut h, &[("y", "6"), ("h", "3")]), None);
    assert_eq!(place(&mut h, &[("x", "30")]), None);
    assert_eq!(place(&mut h, &[("w", "0")]), None);
}

#[test]
fn inline_svg_follows_its_preserve_aspect_ratio() {
    // A square viewBox in a 300x40 box: `none` stretches it across the box,
    // the default fits it inside, centred (SVG 2, 8.6).
    let red_at = |par: &str, x: u32| {
        let mut h = host();
        h.handle(&cmd(
            &[("a", "doc"), ("s", "x")],
            &format!(
                "<body style='margin:0'><svg viewBox='0 0 10 10' {par} \
                 style='display:block;width:300px;height:40px'>\
                 <rect width='10' height='10' fill='#ff0000'/></svg></body>"
            ),
        ));
        h.handle(&cmd(&[("a", "place"), ("s", "x"), ("c", "30"), ("r", "2")], ""));
        render(&mut h);
        let f = h.frame("x").unwrap();
        let i = ((20 * f.width + x) * 4) as usize;
        f.rgba[i] > 200 && f.rgba[i + 1] < 50
    };
    assert!(red_at("preserveAspectRatio='none'", 5));
    assert!(red_at("preserveAspectRatio='none'", 295));
    assert!(!red_at("", 5));
    assert!(red_at("", 150));
}

#[test]
fn hide_removes_the_placement_and_keeps_the_document() {
    let mut h = host();
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], "<p id=p>one</p><input id=i>"));
    h.handle(&cmd(&[("a", "place"), ("s", "x"), ("c", "30"), ("r", "2")], ""));
    render(&mut h);
    h.handle(&cmd(&[("a", "focus"), ("s", "x"), ("t", "i"), ("q", "2")], ""));
    let fx = h.handle(&cmd(&[("a", "hide"), ("s", "x")], ""));
    // The keyboard goes back to the terminal, and the placement goes.
    assert!(fx.contains(&Effect::Hide { surface: "x".into() }), "{fx:?}");
    assert!(replies(&fx).iter().any(|r| r.get("e") == Some("blur")));
    assert_eq!(h.placement("x"), None);
    // Hidden: patches apply, nothing renders.
    h.handle(&cmd(&[("a", "patch"), ("s", "x"), ("op", "text"), ("t", "p"), ("q", "2")], "two"));
    assert!(render(&mut h).is_empty());
    assert_eq!(h.inspect("x", "p").unwrap()["text"], "two");
    // Hiding again does nothing; placing again shows it as it is now.
    assert!(!h.handle(&cmd(&[("a", "hide"), ("s", "x"), ("q", "2")], "")).iter().any(|e| matches!(e, Effect::Hide { .. })));
    h.handle(&cmd(&[("a", "place"), ("s", "x"), ("c", "30"), ("r", "2")], ""));
    assert_eq!(render(&mut h).len(), 1);
}

#[test]
fn a_patch_to_an_element_whose_layout_box_is_gone_repaints_instead_of_panicking() {
    // A tooltip shown on hover lays its <b> out in an anonymous box; hidden
    // again, the box is freed while the <b>, not laid out, still points at
    // it. A patch to the <b> before the next layout used to measure it
    // through that box and panic (a chart's tooltips, in hotty-demo).
    let mut h = host();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x")],
        "<style>body{margin:0} .b{position:absolute;left:0;top:0;width:100px;height:100px}
         .tip{display:none} .b:hover .tip{display:block}</style>
         <div class=b><div class=tip><b id=t>12:00</b><div>row</div></div></div>",
    ));
    h.handle(&cmd(&[("a", "place"), ("s", "x"), ("c", "30"), ("r", "10")], ""));
    render(&mut h);
    h.pointer("x", PointerKind::Move, 50.0, 50.0, Mods::default());
    render(&mut h);
    h.pointer("x", PointerKind::Leave, 0.0, 0.0, Mods::default());
    render(&mut h);
    let r = replies(&h.handle(&cmd(&[("a", "patch"), ("s", "x"), ("op", "text"), ("t", "t")], "12:01")));
    assert_eq!(r[0].get("a"), Some("ok"));
    assert_eq!(render(&mut h).len(), 1);
}

#[test]
fn a_text_patch_damages_only_part_of_the_frame() {
    let mut h = host();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x")],
        "<p id=a style='margin:0'>one</p><p style='margin:0'>two</p><p style='margin:0'>three</p>",
    ));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "100"), ("r", "20")],
        "",
    ));
    render(&mut h);
    let r = replies(&h.handle(&cmd(
        &[("a", "patch"), ("s", "x"), ("op", "text"), ("t", "a")],
        "ONE",
    )));
    assert_eq!(r[0].get("a"), Some("ok"));
    let frames = render(&mut h);
    let hotty_blitz::Damage::Rects(rects) = &frames[0].3 else {
        panic!("expected partial damage, got {:?}", frames[0].3)
    };
    // Only the first paragraph (plus a margin) is repainted, out of 1000x400.
    assert!(rects.iter().all(|r| r.y == 0 && r.h <= 40), "{rects:?}");
    let area: u32 = rects.iter().map(|r| r.w * r.h).sum();
    assert!(area <= 1000 * 40, "{rects:?}");
}

#[test]
fn a_missing_target_is_an_error_and_quiet_suppresses_ok() {
    let mut h = host();
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], "<p id=a>1</p>"));
    let r = replies(&h.handle(&cmd(
        &[("a", "patch"), ("s", "x"), ("op", "text"), ("t", "nope")],
        "2",
    )));
    assert_eq!(r[0].get("a"), Some("err"));
    assert!(String::from_utf8_lossy(&r[0].payload).contains("ENOTARGET"));
    let r = replies(&h.handle(&cmd(
        &[
            ("a", "patch"),
            ("s", "x"),
            ("op", "text"),
            ("t", "a"),
            ("q", "1"),
        ],
        "2",
    )));
    assert!(r.is_empty());
}

fn place_form(h: &mut Host) {
    h.handle(&cmd(
        &[("a", "doc"), ("s", "f")],
        "<style>*{margin:0;padding:0;border:0} input,button{display:block;width:200px;height:20px}</style>
         <div id=list><input id=name value=x><p id=count>1</p>
         <button id=go style='position:absolute;left:0;top:60px;width:100px;height:20px'>Go</button></div>",
    ));
    h.handle(&cmd(
        &[("a", "place"), ("s", "f"), ("c", "30"), ("r", "5")],
        "",
    ));
    render(h);
}

fn type_text(h: &mut Host, s: &str) {
    for c in s.chars() {
        let out = h.key(
            "f",
            &Key {
                name: KeyName::Char(c.to_string()),
                mods: Mods::default(),
            },
        );
        assert!(out.consumed);
    }
}

#[test]
fn morph_keeps_what_the_user_typed_and_change_comes_on_blur() {
    let mut h = host();
    place_form(&mut h);
    h.handle(&cmd(&[("a", "focus"), ("s", "f"), ("t", "name")], ""));
    type_text(&mut h, "yz");
    // An immediate-mode program re-sends the whole tree with a new count.
    let r = replies(&h.handle(&cmd(
        &[("a", "patch"), ("s", "f"), ("op", "morph"), ("t", "list")],
        "<div id=list><input id=name value=x><p id=count>2</p>
         <button id=go style='position:absolute;left:0;top:60px;width:100px;height:20px'>Go</button></div>",
    )));
    assert_eq!(r[0].get("a"), Some("ok"));
    // Typing costs no round trip: nothing has been reported yet.
    let fx = h.blur("f");
    let ev = replies(&fx);
    let change = ev
        .iter()
        .find(|c| c.get("e") == Some("change"))
        .expect("change on blur");
    assert_eq!(change.get("t"), Some("name"));
    let v: serde_json::Value = serde_json::from_slice(&change.payload).unwrap();
    assert!(v["value"].as_str().unwrap().contains("yz"), "{v}");
}

#[test]
fn clicking_a_button_reports_its_id() {
    let mut h = host();
    place_form(&mut h);
    let mut fx = h.pointer("f", PointerKind::Down, 10.0, 65.0, Mods::default());
    fx.extend(h.pointer("f", PointerKind::Up, 10.0, 65.0, Mods::default()));
    let ev = replies(&fx);
    assert!(
        ev.iter()
            .any(|c| c.get("e") == Some("click") && c.get("t") == Some("go")),
        "{ev:?}"
    );
}

#[test]
fn links_report_href_and_url_with_or_without_an_id() {
    let mut h = host();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "l"), ("q", "2")],
        r#"<base href="https://example.com/blog/"><style>body{margin:0} a{display:block;height:40px}</style>
<a href="../about">About</a><a id=ext href="https://other.org/x">Other</a>"#,
    ));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "l"),
            ("c", "20"),
            ("r", "5"),
            ("q", "2"),
        ],
        "",
    ));
    render(&mut h);
    let click = |h: &mut Host, y: f32| {
        let mut fx = h.pointer("l", PointerKind::Down, 10.0, y, Mods::default());
        fx.extend(h.pointer("l", PointerKind::Up, 10.0, y, Mods::default()));
        replies(&fx)
            .into_iter()
            .find(|c| c.get("e") == Some("click"))
            .expect("a click event")
    };
    let first = click(&mut h, 10.0);
    assert_eq!(first.get("t"), Some(""));
    let detail: serde_json::Value = serde_json::from_slice(&first.payload).unwrap();
    assert_eq!(
        detail,
        serde_json::json!({"href": "../about", "url": "https://example.com/about"})
    );
    let second = click(&mut h, 50.0);
    assert_eq!(second.get("t"), Some("ext"));
    let detail: serde_json::Value = serde_json::from_slice(&second.payload).unwrap();
    assert_eq!(
        detail,
        serde_json::json!({"href": "https://other.org/x", "url": "https://other.org/x"})
    );

    // This host fetches nothing from the network (SPEC §7.2), and says so.
    let r = replies(&h.handle(&cmd(&[("a", "q"), ("n", "1")], "")));
    let caps: serde_json::Value = serde_json::from_slice(&r[0].payload).unwrap();
    assert_eq!(caps["net"], serde_json::json!({}));
}

#[test]
fn keys_the_surface_does_not_use_go_to_the_program() {
    let mut h = host();
    place_form(&mut h);
    // Without the keyboard, nothing is consumed.
    let q = Key {
        name: KeyName::Char("q".into()),
        mods: Mods::default(),
    };
    assert!(!h.key("f", &q).consumed);
    h.handle(&cmd(&[("a", "focus"), ("s", "f"), ("t", "go")], ""));
    // A button has no use for 'q' or Escape.
    assert!(!h.key("f", &q).consumed);
    let esc = Key {
        name: KeyName::Escape,
        mods: Mods::default(),
    };
    assert!(!h.key("f", &esc).consumed);
}

#[test]
fn a_stylesheet_resource_arrives_after_the_document() {
    let mut h = host();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x")],
        "<link rel=stylesheet href='cid:theme'><p class=t>hello</p>",
    ));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "20"), ("r", "2")],
        "",
    ));
    assert!(
        render(&mut h).is_empty(),
        "painting waits for the stylesheet"
    );
    let r = replies(&h.handle(&cmd(
        &[("a", "res"), ("id", "theme"), ("type", "text/css")],
        "p.t{color:red}",
    )));
    assert_eq!(r[0].get("a"), Some("ok"));
    let frames = render(&mut h);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].1, 200);
}

#[test]
fn other_schemes_fail_closed() {
    let mut h = host();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x")],
        "<link rel=stylesheet href='https://example.com/x.css'><img src='file:///etc/passwd'><p>ok</p>",
    ));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "20"), ("r", "2")],
        "",
    ));
    // Renders (the blocked stylesheet does not hold painting forever).
    assert_eq!(render(&mut h).len(), 1);
}

#[test]
fn submitting_a_form_reports_its_fields() {
    // A form with an empty action (the usual case here) used to panic Blitz
    // while resolving the action against a document with no base URL.
    let mut h = host();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "f")],
        "<style>*{margin:0;padding:0;border:0}input,button{display:block;width:200px;height:20px}</style>
         <form id=settings><input id=name name=name value=x><input type=checkbox id=ok name=ok value=yes>
         <button type=submit id=save>Save</button></form>",
    ));
    h.handle(&cmd(
        &[("a", "place"), ("s", "f"), ("c", "30"), ("r", "5")],
        "",
    ));
    render(&mut h);
    h.handle(&cmd(&[("a", "focus"), ("s", "f"), ("t", "name")], ""));
    type_text(&mut h, "yz");
    let mut fx = Vec::new();
    for (x, y) in [(5.0, 25.0), (10.0, 45.0)] {
        fx.extend(h.pointer("f", PointerKind::Down, x, y, Mods::default()));
        fx.extend(h.pointer("f", PointerKind::Up, x, y, Mods::default()));
        render(&mut h);
    }
    let ev = replies(&fx);
    let kinds: Vec<_> = ev
        .iter()
        .map(|c| {
            (
                c.get("e").unwrap_or("").to_string(),
                c.get("t").unwrap_or("").to_string(),
            )
        })
        .collect();
    assert!(kinds.contains(&("change".into(), "ok".into())), "{kinds:?}");
    assert!(
        kinds.contains(&("change".into(), "name".into())),
        "{kinds:?}"
    );
    assert!(
        kinds.contains(&("click".into(), "save".into())),
        "{kinds:?}"
    );
    let submit = ev
        .iter()
        .find(|c| c.get("e") == Some("submit"))
        .expect("submit");
    assert_eq!(submit.get("t"), Some("settings"));
    let fields: serde_json::Value = serde_json::from_slice(&submit.payload).unwrap();
    assert_eq!(fields["ok"], "yes");
    assert!(fields["name"].as_str().unwrap().contains("yz"), "{fields}");
}

/// Damage-only painting must produce exactly what a full paint of the same
/// document produces, whatever the patches did to layout.
#[test]
fn partial_repaint_matches_a_full_paint() {
    let doc = "<style>body{margin:0;font:14px sans-serif}.row{display:flex;gap:6px;padding:4px}
        .c{background:#345;color:#eee;padding:2px 6px;border-radius:4px;box-shadow:0 2px 6px #000}
        .bar{height:8px;background:#222}.bar>i{display:block;height:100%;width:calc(var(--p,10)*1%);background:#8c4}
        </style>
        <div class=row id=r1><span class=c id=a>alpha</span><span class=c id=b>beta</span><span class=c id=c>gamma</span></div>
        <p id=p style='margin:4px'>short</p>
        <div class=bar><i id=bar></i></div>
        <ul id=list><li id=l1>one</li><li id=l2>two</li><li id=l3>three</li></ul>
        <p id=tail>tail text below everything</p>";
    let patches: Vec<(Vec<(&str, &str)>, &str)> = vec![
        (vec![("op", "text"), ("t", "a")], "alpha-longer-now"),
        (vec![("op", "var"), ("t", "bar"), ("k", "p")], "73"),
        (
            vec![("op", "attr"), ("t", "b"), ("k", "style")],
            "background:#a33",
        ),
        (
            vec![("op", "text"), ("t", "p")],
            "a paragraph that is now long enough to wrap onto a second line in this narrow surface, pushing everything below it down",
        ),
        (vec![("op", "remove"), ("t", "l2")], ""),
        (
            vec![("op", "append"), ("t", "list")],
            "<li id=l4>four</li><li id=l5>five</li>",
        ),
        (
            vec![("op", "morph"), ("t", "c")],
            "<span class=c id=c>GAMMA!</span>",
        ),
    ];
    let place = |h: &mut Host| {
        h.handle(&cmd(
            &[("a", "place"), ("s", "x"), ("c", "40"), ("r", "20")],
            "",
        ));
    };
    // Incremental: first frame, then one render per patch.
    let mut inc = host();
    inc.handle(&cmd(&[("a", "doc"), ("s", "x")], doc));
    place(&mut inc);
    render(&mut inc);
    let mut partial = 0;
    for (ctl, payload) in &patches {
        let mut c: Vec<(&str, &str)> = vec![("a", "patch"), ("s", "x")];
        c.extend(ctl.iter().copied());
        let r = replies(&inc.handle(&cmd(&c, payload)));
        assert_eq!(
            r[0].get("a"),
            Some("ok"),
            "{:?}",
            String::from_utf8_lossy(&r[0].payload)
        );
        for (_, _, _, d) in render(&mut inc) {
            if matches!(d, hotty_blitz::Damage::Rects(_)) {
                partial += 1;
            }
        }
    }
    assert!(
        partial >= 5,
        "most patches should repaint partially ({partial})"
    );
    // Reference: the same patches, one full paint at the end.
    let mut full = host();
    full.handle(&cmd(&[("a", "doc"), ("s", "x")], doc));
    place(&mut full);
    for (ctl, payload) in &patches {
        let mut c: Vec<(&str, &str)> = vec![("a", "patch"), ("s", "x")];
        c.extend(ctl.iter().copied());
        full.handle(&cmd(&c, payload));
    }
    render(&mut full);
    let (a, b) = (inc.frame("x").unwrap(), full.frame("x").unwrap());
    assert_eq!((a.width, a.height), (b.width, b.height));
    let differing = a
        .rgba
        .chunks(4)
        .zip(b.rgba.chunks(4))
        .filter(|(p, q)| p != q)
        .count();
    assert_eq!(differing, 0, "{differing} pixels differ from a full paint");
}

/// A link 20px tall across the top, then prose from y = 24.
const PROSE: &str = "<style>body{margin:0;font:14px sans-serif}p{margin:4px}
    a{display:block;height:20px}</style>
    <a id=go href='/go'>a link</a>
    <p>The first paragraph, long enough to wrap onto a second line here.</p>
    <p>A second one, a little shorter.</p>
    <p>And a third, below the rest.</p>";

#[test]
fn dragging_repaints_the_selection_as_it_grows() {
    let place = |h: &mut Host| {
        h.handle(&cmd(&[("a", "doc"), ("s", "x")], PROSE));
        h.handle(&cmd(
            &[("a", "place"), ("s", "x"), ("c", "30"), ("r", "8")],
            "",
        ));
    };
    let path = [(60.0, 32.0), (200.0, 50.0), (120.0, 75.0), (80.0, 95.0)];
    let mods = Mods::default();
    // Incremental: a frame after every step of the drag.
    let mut inc = host();
    place(&mut inc);
    render(&mut inc);
    let before = inc.frame("x").unwrap().rgba.clone();
    inc.pointer("x", PointerKind::Down, 5.0, 32.0, mods);
    render(&mut inc);
    for (x, y) in path {
        inc.pointer("x", PointerKind::Move, x, y, mods);
        let frames = render(&mut inc);
        assert_eq!(frames.len(), 1, "a step of the drag at ({x}, {y}) repaints");
    }
    // Reference: the same drag, painted once at the end (the press repaints
    // everything; the first frame lays the document out to hit it).
    let mut full = host();
    place(&mut full);
    render(&mut full);
    full.pointer("x", PointerKind::Down, 5.0, 32.0, mods);
    for (x, y) in path {
        full.pointer("x", PointerKind::Move, x, y, mods);
    }
    render(&mut full);
    let (a, b) = (inc.frame("x").unwrap(), full.frame("x").unwrap());
    assert_ne!(a.rgba, before, "the drag selected something");
    let differing = a
        .rgba
        .chunks(4)
        .zip(b.rgba.chunks(4))
        .filter(|(p, q)| p != q)
        .count();
    assert_eq!(differing, 0, "{differing} pixels differ from a full paint");
}

#[test]
fn the_pointer_shape_follows_the_document() {
    let mut h = host();
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], PROSE));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "30"), ("r", "8")],
        "",
    ));
    render(&mut h);
    h.pointer("x", PointerKind::Move, 150.0, 10.0, Mods::default());
    assert_eq!(h.cursor("x"), Some("pointer"));
    h.pointer("x", PointerKind::Move, 20.0, 32.0, Mods::default());
    assert_eq!(h.cursor("x"), Some("text"));
    assert_eq!(h.cursor("nothing"), None);
}

/// A page with the layout modes an incremental relayout has to get right:
/// nested content-sized flex-wrap (measured many ways per pass), fractional
/// widths (rounding), grid, a float, a table and absolutely positioned boxes.
/// Each step changes a few of its parts, so relayout stays incremental.
fn busy_page(step: usize) -> String {
    // A part's content changes only on the steps that name it.
    let v = |part: usize| (0..=step).filter(|s| s % 11 == part % 11).count();
    // Cells change one or two at a time.
    let cell = |part: usize, k: usize| (0..=step).filter(|s| (s * k) % 96 == part).count();
    let mut s = String::from("<main id=m>");
    s += &format!(
        "<div class=abs style='left:{}px'>badge {}</div>",
        10 + v(3) * 7,
        v(3)
    );
    s += "<div class=wrap>";
    for g in 0..12 {
        s += "<div class=g>";
        for c in 0..8 {
            let part = g * 8 + c;
            let text = "x".repeat((part * 7 + cell(part, 13) * 3) % 9 + 1);
            let wide = if cell(part, 29) % 2 == 1 { " wide" } else { "" };
            s += &format!("<span class='c{wide}'>{text}</span>");
        }
        if v(g + 1) % 2 == 1 {
            s += "<span class=c>extra</span>";
        }
        s += "</div>";
    }
    s += "</div>";
    s += &format!(
        "<p class=f><b class=fl>float {}</b>{}</p>",
        v(7),
        "words that wrap around a float ".repeat(v(7) % 3 + 1)
    );
    s += "<div class=grid>";
    for i in 0..9 {
        s += &format!("<div>{}</div>", "g".repeat((i * 3 + v(i + 2)) % 7 + 1));
    }
    s += "</div><table>";
    for r in 0..3 {
        s += &format!(
            "<tr><td>{}</td><td>{}</td></tr>",
            "t".repeat((r + v(r + 4)) % 5 + 1),
            r * v(9)
        );
    }
    s += "</table>";
    // Items whose baseline moves while their box may not.
    s += "<div class=base>";
    for i in 0..6 {
        let big = if cell(40 + i, 17) % 2 == 1 { "big" } else { "" };
        s += &format!("<span class='{big}'>b{i}</span>");
    }
    // A column whose items share out a fixed height.
    s += "</div><div class=col>";
    for i in 0..4 {
        s += &format!(
            "<div style='flex-grow:{}'>{}</div>",
            1 + cell(50 + i, 19) % 3,
            "c".repeat(cell(54 + i, 5) % 4 + 1)
        );
    }
    // Fixed-size clipped cells, as in `hotty bench`.
    s += "</div><div class=cells>";
    for i in 0..40 {
        s += &format!(
            "<span class=k>{}</span>",
            (i * 7 + cell(56 + i, 23) * 13) % 1000
        );
    }
    s += "</div></main>";
    s
}

/// The whole incremental path (Blitz's damage, the fork's construction
/// skip, layout cache, speculative relayout and incremental rounding, then
/// hotty-blitz's partial paint) against the same page built from scratch, step
/// after step. A debug build also checks every incremental rounding against
/// a full one.
#[test]
fn incremental_layout_matches_a_fresh_document() {
    let style = "<style>body{margin:0;font:13px sans-serif}main{position:relative}
        .abs{position:absolute;top:3px;background:#a33;color:#fff}
        .wrap{display:flex;flex-wrap:wrap;gap:1.5px;padding:2.25px}
        .g{display:flex;flex-wrap:wrap;gap:1px;width:calc(100% / 3 - 2px);height:61.5px;overflow:hidden;outline:1px solid #555}
        .c{display:inline-block;padding:0 1.3px;background:#345;color:#eee}.wide{padding:0 7.7px}
        .fl{float:left;width:33.3%;background:#553}
        .grid{display:grid;grid-template-columns:repeat(3,1fr);gap:0.7px}
        td{border:1px solid #666;padding:0.4px 2px}
        .base{display:flex;align-items:baseline;gap:3px}.big{font-size:19px}
        .col{display:flex;flex-direction:column;height:83px;width:50%;background:#223}
        .col>div{border-bottom:1px solid #556}
        .cells{display:flex;flex-wrap:wrap;gap:1px}
        .k{width:33px;height:15.5px;overflow:hidden;background:#333;text-align:center}</style>";
    let place = |h: &mut Host| {
        h.handle(&cmd(
            &[("a", "place"), ("s", "x"), ("c", "60"), ("r", "50")],
            "",
        ));
    };
    let mut inc = host();
    inc.handle(&cmd(
        &[("a", "doc"), ("s", "x")],
        &(style.to_string() + &busy_page(0)),
    ));
    place(&mut inc);
    render(&mut inc);
    for step in 1..23 {
        let r = replies(&inc.handle(&cmd(
            &[("a", "patch"), ("s", "x"), ("op", "morph"), ("t", "m")],
            &busy_page(step),
        )));
        assert_eq!(r[0].get("a"), Some("ok"));
        render(&mut inc);
        let partial = inc.frame("x").unwrap().rgba.clone();
        inc.repaint();
        render(&mut inc);
        // A partial paint draws the same scene from another origin, and
        // vello_cpu's f32 coverage can then differ by one level on an
        // anti-aliased edge at a fractional position.
        let whole = &inc.frame("x").unwrap().rgba;
        let unpainted = partial
            .iter()
            .zip(whole.iter())
            .filter(|(p, q)| p.abs_diff(**q) > 1)
            .count();
        assert_eq!(
            unpainted, 0,
            "step {step}: {unpainted} channels differ from a full paint"
        );

        let mut fresh = host();
        fresh.handle(&cmd(
            &[("a", "doc"), ("s", "x")],
            &(style.to_string() + &busy_page(step)),
        ));
        place(&mut fresh);
        render(&mut fresh);
        let (a, b) = (inc.frame("x").unwrap(), fresh.frame("x").unwrap());
        assert_eq!((a.width, a.height), (b.width, b.height));
        let differing = a
            .rgba
            .chunks(4)
            .zip(b.rgba.chunks(4))
            .filter(|(p, q)| p != q)
            .count();
        assert_eq!(
            differing, 0,
            "step {step}: {differing} pixels differ from a fresh document"
        );
    }
}

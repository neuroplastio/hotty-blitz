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
fn an_outline_takes_its_offset_and_style() {
    // A surface's root, outlined inside its own box: dashes over the root's
    // background, along the surface's edges. Upstream drew every outline
    // solid, outside the border box (off the surface), under the background.
    let mut h = host();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x")],
        "<body style='margin:0'><div style='height:40px;background:#0000ff;\
         outline:2px dashed #ff0000;outline-offset:-2px'></div></body>",
    ));
    h.handle(&cmd(&[("a", "place"), ("s", "x"), ("c", "30"), ("r", "2")], ""));
    render(&mut h);
    let f = h.frame("x").unwrap();
    let red = |x: u32, y: u32| {
        let i = ((y * f.width + x) * 4) as usize;
        f.rgba[i] > 200 && f.rgba[i + 2] < 50
    };
    let (top, left): (Vec<bool>, Vec<bool>) = (
        (0..f.width).map(|x| red(x, 0)).collect(),
        (0..f.height).map(|y| red(0, y)).collect(),
    );
    for edge in [&top, &left] {
        assert!(edge.iter().any(|&r| r) && edge.iter().any(|&r| !r), "{edge:?}");
    }
    // A dash at each end of an edge (beside the corner, whose pixel the two
    // edges' mitre shares).
    assert!(red(1, 0) && red(f.width - 2, f.height - 1));
    assert!(!red(3, 3) && !red(f.width / 2, f.height / 2));
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
fn a_hyperlink_is_the_terminals_and_reports_nothing() {
    let mut h = host();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "l"), ("q", "2")],
        r#"<base href="https://example.com/blog/"><style>body{margin:0} a{display:block;height:40px}</style>
<a id=out target=_blank href="../spec">Spec</a><a id=in href="../about">About</a>"#,
    ));
    h.handle(&cmd(
        &[("a", "place"), ("s", "l"), ("c", "20"), ("r", "5"), ("q", "2")],
        "",
    ));
    render(&mut h);
    let click = |h: &mut Host, y: f32| {
        let mut fx = h.pointer("l", PointerKind::Down, 10.0, y, Mods::default());
        fx.extend(h.pointer("l", PointerKind::Up, 10.0, y, Mods::default()));
        replies(&fx)
            .into_iter()
            .filter(|c| c.get("e") == Some("click"))
            .count()
    };
    h.pointer("l", PointerKind::Move, 10.0, 10.0, Mods::default());
    assert_eq!(h.hyperlink("l").as_deref(), Some("https://example.com/spec"));
    assert_eq!(click(&mut h, 10.0), 0, "a hyperlink's click is not reported");
    h.pointer("l", PointerKind::Move, 10.0, 50.0, Mods::default());
    assert_eq!(h.hyperlink("l"), None, "a link of the program's is no hyperlink");
    assert_eq!(click(&mut h, 50.0), 1);
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

/// A click (press and release) at `(x, y)`: every HOTTY message it produced.
fn click(h: &mut Host, s: &str, x: f32, y: f32) -> Vec<Command> {
    let mut fx = h.pointer(s, PointerKind::Down, x, y, Mods::default());
    fx.extend(h.pointer(s, PointerKind::Up, x, y, Mods::default()));
    replies(&fx)
}

/// `(e, t)` of each event.
fn kinds(ev: &[Command]) -> Vec<(String, String)> {
    ev.iter()
        .filter(|c| c.get("a") == Some("ev"))
        .map(|c| {
            (
                c.get("e").unwrap_or("").to_string(),
                c.get("t").unwrap_or("").to_string(),
            )
        })
        .collect()
}

fn ev(e: &str, t: &str) -> (String, String) {
    (e.to_string(), t.to_string())
}

fn key(h: &mut Host, s: &str, c: &str) -> hotty_blitz::KeyOutcome {
    h.key(
        s,
        &Key {
            name: KeyName::Char(c.into()),
            mods: Mods::default(),
        },
    )
}

/// The pixel at `(x, y)` of the surface's last frame, as RGB.
fn pixel(h: &Host, s: &str, x: u32, y: u32) -> [u8; 3] {
    let f = h.frame(s).unwrap();
    let i = ((y * f.width + x) * 4) as usize;
    [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
}

fn doc_at(h: &mut Host, s: &str, extra: &[(&str, &str)], html: &str, rows: &str) {
    let mut pairs = vec![("a", "doc"), ("s", s), ("q", "2")];
    pairs.extend_from_slice(extra);
    h.handle(&cmd(&pairs, html));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", s),
            ("c", "30"),
            ("r", rows),
            ("q", "2"),
        ],
        "",
    ));
    render(h);
}

#[test]
fn clicking_the_text_of_an_inert_document_takes_nothing() {
    // The bug: Blitz calls the root element focused when nothing is, so a
    // click on text took the keyboard and sent `focus`, and the click away
    // sent `blur`. A shell under the document read both as typed text.
    let mut h = host();
    doc_at(&mut h, "x", &[], "<p>just text</p>", "3");
    for (x, y) in [(20.0, 25.0), (250.0, 50.0)] {
        assert_eq!(click(&mut h, "x", x, y), vec![], "a click at ({x}, {y})");
        assert!(!h.is_focused("x"));
    }
    assert_eq!(h.focused_surface(), None);
    // The terminal's click elsewhere: nothing to give back.
    assert!(h.blur("x").is_empty());
}

#[test]
fn a_click_takes_the_keyboard_only_through_an_element_that_takes_focus() {
    let mut h = host();
    place_form(&mut h);
    // A button takes focus, and with it the keyboard.
    let k = kinds(&click(&mut h, "f", 10.0, 65.0));
    assert_eq!(k, vec![ev("focus", ""), ev("click", "go")]);
    assert!(h.is_focused("f"));
    // Text does not: a click on it gives the keyboard back.
    assert_eq!(kinds(&click(&mut h, "f", 10.0, 30.0)), vec![ev("blur", "")]);
    assert!(!h.is_focused("f"));
    assert_eq!(kinds(&click(&mut h, "f", 250.0, 90.0)), vec![]);
}

#[test]
fn a_click_inside_on_what_takes_no_focus_commits_and_gives_the_keyboard_back() {
    let mut h = host();
    place_form(&mut h);
    h.handle(&cmd(
        &[("a", "focus"), ("s", "f"), ("t", "name"), ("q", "2")],
        "",
    ));
    type_text(&mut h, "yz");
    // The paragraph under the field takes no focus: the field commits its
    // value, then the surface sends blur.
    let got = click(&mut h, "f", 10.0, 30.0);
    assert_eq!(kinds(&got), vec![ev("change", "name"), ev("blur", "")]);
    let v: serde_json::Value = serde_json::from_slice(&got[0].payload).unwrap();
    assert!(v["value"].as_str().unwrap().contains("yz"), "{v}");
    assert!(!h.is_focused("f"));
    assert!(!key(&mut h, "f", "q").consumed);
}

#[test]
fn a_press_on_another_surface_takes_the_keyboard_from_this_one() {
    let mut h = host();
    place_form(&mut h);
    h.handle(&cmd(
        &[("a", "focus"), ("s", "f"), ("t", "name"), ("q", "2")],
        "",
    ));
    doc_at(&mut h, "x", &[("d", "1")], "<p>printed</p>", "2");
    let fx = h.pointer("x", PointerKind::Down, 20.0, 25.0, Mods::default());
    let got = replies(&fx);
    assert!(got.iter().all(|c| c.get("s") == Some("f")), "{got:?}");
    assert_eq!(kinds(&got), vec![ev("blur", "")]);
    assert_eq!(h.focused_surface(), None);
}

/// A form with every kind of control, a link and an element that asks
/// for clicks, one per 20 px row.
const CONTROLS: &str = "<style>*{margin:0;padding:0;border:0}
    input,button,a,div{display:block;width:200px;height:20px}</style>
    <form id=f><input id=name name=name value=x><input type=checkbox id=ok name=ok value=yes>
    <button type=submit id=save>Save</button></form>
    <a id=go href='/go'>go</a><div id=d data-on=click>tap</div>
    <details><summary id=more>more</summary><p>hidden</p></details>";

#[test]
fn a_detached_surface_sends_no_events() {
    let mut h = host();
    doc_at(&mut h, "x", &[], CONTROLS, "8");
    // The same clicks on the attached surface report.
    let mut attached = host();
    doc_at(&mut attached, "x", &[], CONTROLS, "8");
    let mut heard = Vec::new();
    for y in [5.0, 25.0, 45.0, 65.0, 85.0, 105.0] {
        heard.extend(kinds(&click(&mut attached, "x", 10.0, y)));
    }
    for e in ["click", "change", "submit", "focus", "blur"] {
        assert!(heard.iter().any(|(k, _)| k == e), "no {e} in {heard:?}");
    }

    let r = replies(&h.handle(&cmd(&[("a", "detach"), ("s", "x")], "")));
    assert_eq!(r.len(), 1);
    assert_eq!(
        (r[0].get("a"), r[0].get("re")),
        (Some("ok"), Some("detach"))
    );
    let mut fx = Vec::new();
    for y in [5.0, 25.0, 45.0, 65.0, 85.0, 105.0, 125.0] {
        fx.extend(h.pointer("x", PointerKind::Move, 10.0, y, Mods::default()));
        fx.extend(h.pointer("x", PointerKind::Down, 10.0, y, Mods::default()));
        fx.extend(h.pointer("x", PointerKind::Up, 10.0, y, Mods::default()));
        render(&mut h);
        let k = key(&mut h, "x", "q");
        assert!(!k.consumed);
        fx.extend(k.effects);
    }
    fx.extend(h.pointer("x", PointerKind::Leave, 0.0, 0.0, Mods::default()));
    fx.extend(h.blur("x"));
    fx.extend(h.handle(&cmd(&[("a", "blur"), ("s", "x"), ("q", "2")], "")));
    fx.extend(h.handle(&cmd(&[("a", "hide"), ("s", "x"), ("q", "2")], "")));
    // hotty-blitz sends no `resize` at all: its pixel size follows its cells.
    assert_eq!(replies(&fx), vec![]);
    assert_eq!(h.focused_surface(), None);
}

#[test]
fn detaching_gives_the_keyboard_back_without_change_or_blur() {
    let mut h = host();
    place_form(&mut h);
    h.handle(&cmd(
        &[("a", "focus"), ("s", "f"), ("t", "name"), ("q", "2")],
        "",
    ));
    type_text(&mut h, "yz");
    let r = replies(&h.handle(&cmd(&[("a", "detach"), ("s", "f")], "")));
    assert_eq!(r.len(), 1, "{r:?}");
    assert_eq!(
        (r[0].get("a"), r[0].get("re")),
        (Some("ok"), Some("detach"))
    );
    assert!(!h.is_focused("f"));
    assert_eq!(h.focused_surface(), None);
    let k = key(&mut h, "f", "q");
    assert!(!k.consumed && k.effects.is_empty());
    // It never has the keyboard again, nor sends what it held back.
    let r = replies(&h.handle(&cmd(&[("a", "focus"), ("s", "f"), ("t", "name")], "")));
    assert!(String::from_utf8_lossy(&r[0].payload).contains("EDETACHED"));
    assert!(h.blur("f").is_empty());
    assert_eq!(kinds(&click(&mut h, "f", 10.0, 5.0)), vec![]);
}

/// Markers that show the controls' state: blue while one is disabled, red
/// once it is checked or focused, black otherwise.
const MARKED: &str = "<style>*{margin:0;padding:0;border:0} body{background:#000}
    input{display:block;width:100px;height:20px} .m{width:20px;height:20px}
    #cb:disabled ~ #m1, #t:disabled ~ #m2, #cb2:disabled + #m3 {background:#00f}
    #cb:checked ~ #m1, #t:focus ~ #m2, #cb2:checked + #m3 {background:#f00}</style>
    <div id=box><input type=checkbox id=cb><input id=t value=x>
    <div class=m id=m1></div><div class=m id=m2></div></div>";

const BLUE: [u8; 3] = [0, 0, 255];
const RED: [u8; 3] = [255, 0, 0];

#[test]
fn a_detached_surfaces_controls_are_disabled() {
    // Attached, a click checks the box and focuses the field.
    let mut h = host();
    doc_at(&mut h, "x", &[], MARKED, "6");
    click(&mut h, "x", 10.0, 10.0);
    click(&mut h, "x", 10.0, 30.0);
    render(&mut h);
    assert_eq!((pixel(&h, "x", 10, 50), pixel(&h, "x", 10, 70)), (RED, RED));

    let mut h = host();
    doc_at(&mut h, "x", &[], MARKED, "6");
    h.handle(&cmd(&[("a", "detach"), ("s", "x"), ("q", "2")], ""));
    render(&mut h);
    // They match :disabled, and a click neither toggles nor focuses them.
    assert_eq!(
        (pixel(&h, "x", 10, 50), pixel(&h, "x", 10, 70)),
        (BLUE, BLUE)
    );
    click(&mut h, "x", 10.0, 10.0);
    click(&mut h, "x", 10.0, 30.0);
    assert!(!key(&mut h, "x", "q").consumed, "typing does not edit");
    render(&mut h);
    assert_eq!(
        (pixel(&h, "x", 10, 50), pixel(&h, "x", 10, 70)),
        (BLUE, BLUE)
    );
    // The document keeps the program's attributes.
    let attrs = &h.inspect("x", "cb").unwrap()["attrs"];
    assert_eq!(*attrs, serde_json::json!({"type": "checkbox", "id": "cb"}));

    // A control a patch adds is disabled too, and so is one a patch takes
    // `disabled` from.
    let r = replies(&h.handle(&cmd(
        &[("a", "patch"), ("s", "x"), ("op", "append"), ("t", "box")],
        "<input type=checkbox id=cb2 disabled><div class=m id=m3></div>",
    )));
    assert_eq!(r[0].get("a"), Some("ok"));
    h.handle(&cmd(
        &[
            ("a", "patch"),
            ("s", "x"),
            ("op", "unattr"),
            ("t", "cb2"),
            ("k", "disabled"),
            ("q", "2"),
        ],
        "",
    ));
    render(&mut h);
    assert_eq!(pixel(&h, "x", 10, 110), BLUE);
    click(&mut h, "x", 10.0, 90.0);
    render(&mut h);
    assert_eq!(pixel(&h, "x", 10, 110), BLUE);
    let attrs = &h.inspect("x", "cb2").unwrap()["attrs"];
    assert_eq!(*attrs, serde_json::json!({"type": "checkbox", "id": "cb2"}));
}

#[test]
fn a_detached_document_paints_as_if_its_controls_had_disabled() {
    let controls = |disabled: &str| {
        format!(
            "<style>body{{margin:0}} :disabled{{opacity:0.5}}</style>
             <input type=checkbox checked {disabled}><input type=radio {disabled}>
             <button {disabled}>go</button><input value=text {disabled}>"
        )
    };
    let mut detached = host();
    doc_at(&mut detached, "x", &[("d", "1")], &controls(""), "3");
    let mut disabled = host();
    doc_at(&mut disabled, "x", &[], &controls("disabled"), "3");
    let mut enabled = host();
    doc_at(&mut enabled, "x", &[], &controls(""), "3");
    let rgba = |h: &Host| h.frame("x").unwrap().rgba.clone();
    assert_eq!(rgba(&detached), rgba(&disabled));
    assert_ne!(rgba(&detached), rgba(&enabled));
}

#[test]
fn what_is_local_stays_on_a_detached_surface() {
    let mut h = host();
    doc_at(
        &mut h,
        "l",
        &[("d", "1")],
        r#"<base href="https://example.com/blog/"><style>body{margin:0} a,p,summary{display:block;height:20px;margin:0}</style>
<p>select these words</p><a id=out target=_blank href="../spec">Spec</a><a id=in href="../about">About</a>
<details><summary id=s>more</summary><p>hidden</p></details>"#,
        "6",
    );
    // A hyperlink is the terminal's, and still opens; a link of the
    // program's leads nowhere now, and shows the text pointer.
    h.pointer("l", PointerKind::Move, 10.0, 30.0, Mods::default());
    assert_eq!(
        h.hyperlink("l").as_deref(),
        Some("https://example.com/spec")
    );
    assert_eq!(h.cursor("l"), Some("pointer"));
    assert_eq!(click(&mut h, "l", 10.0, 30.0), vec![]);
    h.pointer("l", PointerKind::Move, 10.0, 50.0, Mods::default());
    assert_eq!(h.hyperlink("l"), None);
    assert_eq!(h.cursor("l"), Some("text"));
    assert_eq!(click(&mut h, "l", 10.0, 50.0), vec![]);

    // Selecting text.
    render(&mut h);
    let before = h.frame("l").unwrap().rgba.clone();
    let mods = Mods::default();
    let mut fx = h.pointer("l", PointerKind::Down, 2.0, 10.0, mods);
    fx.extend(h.pointer("l", PointerKind::Move, 120.0, 10.0, mods));
    fx.extend(h.pointer("l", PointerKind::Up, 120.0, 10.0, mods));
    assert!(fx.is_empty());
    render(&mut h);
    assert_ne!(h.frame("l").unwrap().rgba, before, "the drag selected text");

    // Toggling a <details>, which takes no keyboard.
    let rows = |h: &mut Host| {
        h.handle(&cmd(&[("a", "place"), ("s", "l"), ("c", "30")], ""))
            .into_iter()
            .find_map(|e| match e {
                Effect::Place { rows, .. } => Some(rows),
                _ => None,
            })
            .unwrap()
    };
    let closed = rows(&mut h);
    render(&mut h);
    assert_eq!(click(&mut h, "l", 10.0, 70.0), vec![]);
    assert_eq!(rows(&mut h), closed + 1, "the details opened");
    assert!(!h.is_focused("l"));
}

#[test]
fn a_document_without_d_gives_the_surface_back() {
    let mut h = host();
    let form = "<style>*{margin:0;padding:0;border:0} input,button{display:block;width:200px;height:20px}</style>
        <input id=i><button id=b>go</button>";
    doc_at(&mut h, "x", &[("d", "1")], form, "2");
    assert_eq!(click(&mut h, "x", 10.0, 25.0), vec![]);
    // A new document with d=1 stays detached; one without it is attached.
    doc_at(&mut h, "x", &[("d", "1")], form, "2");
    assert_eq!(click(&mut h, "x", 10.0, 25.0), vec![]);
    doc_at(&mut h, "x", &[], form, "2");
    assert_eq!(
        kinds(&click(&mut h, "x", 10.0, 25.0)),
        vec![ev("focus", ""), ev("click", "b")]
    );
    let r = replies(&h.handle(&cmd(&[("a", "focus"), ("s", "x"), ("t", "i")], "")));
    assert_eq!(r[0].get("a"), Some("ok"));
    assert!(key(&mut h, "x", "q").consumed);
}

/// Detaching restyles every control, and a patch to a detached surface
/// disables the controls it brings: partial paints still equal a full one.
#[test]
fn a_detached_surface_paints_partially_as_it_would_in_full() {
    let doc = "<style>body{margin:0;font:14px sans-serif} p{margin:2px}
        input:disabled,button:disabled{background:#444;color:#aaa}
        input:enabled,button:enabled{background:#fff;color:#000}</style>
        <p id=a>alpha</p><div id=list><input id=i1 value=one><button id=b1>one</button></div>
        <p id=z>tail</p>";
    let patches: Vec<(Vec<(&str, &str)>, &str)> = vec![
        (vec![("op", "text"), ("t", "a")], "alpha, longer"),
        (
            vec![("op", "append"), ("t", "list")],
            "<input id=i2 value=two><input type=checkbox id=c2 checked>",
        ),
        (vec![("op", "attr"), ("t", "b1"), ("k", "disabled")], ""),
        (vec![("op", "unattr"), ("t", "b1"), ("k", "disabled")], ""),
        (
            vec![("op", "morph"), ("t", "z")],
            "<p id=z><button id=b3>three</button></p>",
        ),
    ];
    let place = |h: &mut Host| {
        h.handle(&cmd(
            &[("a", "place"), ("s", "x"), ("c", "40"), ("r", "10")],
            "",
        ));
    };
    let apply = |h: &mut Host, render_each: bool| {
        let mut partial = 0;
        for (ctl, payload) in &patches {
            let mut c: Vec<(&str, &str)> = vec![("a", "patch"), ("s", "x")];
            c.extend(ctl.iter().copied());
            let r = replies(&h.handle(&cmd(&c, payload)));
            assert_eq!(
                r[0].get("a"),
                Some("ok"),
                "{:?}",
                String::from_utf8_lossy(&r[0].payload)
            );
            if render_each {
                for (_, _, _, d) in render(h) {
                    partial += matches!(d, hotty_blitz::Damage::Rects(_)) as u32;
                }
            }
        }
        partial
    };
    // Incremental: attached first, then detached, a frame after each step.
    let mut inc = host();
    inc.handle(&cmd(&[("a", "doc"), ("s", "x")], doc));
    place(&mut inc);
    render(&mut inc);
    let attached = inc.frame("x").unwrap().rgba.clone();
    inc.handle(&cmd(&[("a", "detach"), ("s", "x")], ""));
    render(&mut inc);
    assert_ne!(
        inc.frame("x").unwrap().rgba,
        attached,
        "detaching restyled the controls"
    );
    let partial = apply(&mut inc, true);
    assert!(
        partial >= 3,
        "most patches should repaint partially ({partial})"
    );
    // Reference: created detached, the same patches, one full paint.
    let mut full = host();
    full.handle(&cmd(&[("a", "doc"), ("s", "x"), ("d", "1")], doc));
    place(&mut full);
    apply(&mut full, false);
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

#[test]
fn a_hyperlink_takes_no_keyboard() {
    let mut h = host();
    doc_at(
        &mut h,
        "l",
        &[],
        r#"<base href="https://example.com/"><style>*{margin:0;padding:0;border:0} a{display:block;width:200px;height:20px}</style>
<a id=out target=_blank href="/spec">Spec</a><a id=in href="/about">About</a>"#,
        "3",
    );
    // A hyperlink is the terminal's: a click on it takes nothing.
    assert_eq!(click(&mut h, "l", 10.0, 10.0), vec![]);
    assert!(!h.is_focused("l"));
    // A link of the program's takes the keyboard...
    let k = kinds(&click(&mut h, "l", 10.0, 30.0));
    assert_eq!(k, vec![ev("focus", ""), ev("click", "in")]);
    // ...which a click on the hyperlink, taking no focus, gives back.
    assert_eq!(kinds(&click(&mut h, "l", 10.0, 10.0)), vec![ev("blur", "")]);
}

#[test]
fn only_the_first_summary_of_a_details_takes_focus() {
    let mut h = host();
    doc_at(
        &mut h,
        "d",
        &[],
        "<style>*{margin:0;padding:0} summary{display:block;height:20px}</style>
         <details open><summary id=s1>one</summary><summary id=s2>two</summary><p>body</p></details>",
        "4",
    );
    assert_eq!(
        kinds(&click(&mut h, "d", 10.0, 30.0)),
        vec![ev("click", "s2")]
    );
    assert!(!h.is_focused("d"));
    let k = kinds(&click(&mut h, "d", 10.0, 10.0));
    assert_eq!(k, vec![ev("focus", ""), ev("click", "s1")]);
    assert!(h.is_focused("d"));
}

#[test]
fn a_click_on_a_label_is_a_click_on_its_control() {
    let mut h = host();
    doc_at(
        &mut h,
        "f",
        &[],
        "<style>*{margin:0;padding:0;border:0}
         label,input,button{display:block;width:200px;height:20px} label.wrap{height:auto}</style>
         <label for=t>Name</label><input id=t>
         <label class=wrap>Agree <input type=checkbox id=c></label>
         <label for=b>Go</label><button id=b>go</button>
         <label for=x>Off</label><button id=x disabled>off</button>",
        "8",
    );
    // A text field: its label focuses it, and typing goes into it.
    assert_eq!(
        kinds(&click(&mut h, "f", 10.0, 10.0)),
        vec![ev("focus", "")]
    );
    assert!(key(&mut h, "f", "q").consumed);
    h.blur("f");
    // A checkbox: toggled, and focused.
    let got = click(&mut h, "f", 10.0, 50.0);
    assert_eq!(kinds(&got), vec![ev("focus", ""), ev("change", "c")]);
    let v: serde_json::Value = serde_json::from_slice(&got[1].payload).unwrap();
    assert_eq!(v["checked"], true);
    h.blur("f");
    // A button: clicked, and focused.
    let k = kinds(&click(&mut h, "f", 10.0, 90.0));
    assert_eq!(k, vec![ev("focus", ""), ev("click", "b")]);
    h.blur("f");
    // A disabled button: nothing, through its label or not.
    assert_eq!(click(&mut h, "f", 10.0, 130.0), vec![]);
    assert_eq!(click(&mut h, "f", 10.0, 150.0), vec![]);
    assert!(!h.is_focused("f"));
}

#[test]
fn on_a_detached_surface_focus_is_edetached_and_blur_does_nothing() {
    let mut h = host();
    doc_at(&mut h, "x", &[("d", "1")], "<input id=i>", "2");
    for t in [Some("i"), Some("nope"), None] {
        let mut c = vec![("a", "focus"), ("s", "x")];
        c.extend(t.map(|t| ("t", t)));
        let r = replies(&h.handle(&cmd(&c, "")));
        assert_eq!(r.len(), 1, "{t:?}");
        assert!(
            String::from_utf8_lossy(&r[0].payload).contains("EDETACHED"),
            "{t:?}: {:?}",
            r[0].control.encode()
        );
    }
    let r = replies(&h.handle(&cmd(&[("a", "blur"), ("s", "x")], "")));
    assert_eq!(r.len(), 1);
    assert_eq!((r[0].get("a"), r[0].get("re")), (Some("ok"), Some("blur")));
}

#[test]
fn a_detached_surface_shows_a_hand_only_over_a_hyperlink() {
    let doc = r#"<base href="https://example.com/"><style>*{margin:0;padding:0;border:0}
        a,button,div{display:block;width:200px;height:20px}</style>
        <button id=b style="cursor:pointer">button</button>
        <div id=d data-on=click style="cursor:pointer">tap</div>
        <a href="/in">a link</a><div style="cursor:pointer"></div>
        <a target=_blank href="/out">a hyperlink</a>"#;
    let shapes = |extra: &[(&str, &str)]| {
        let mut h = host();
        doc_at(&mut h, "x", extra, doc, "5");
        [10.0, 30.0, 50.0, 70.0, 90.0].map(|y| {
            h.pointer("x", PointerKind::Move, 15.0, y, Mods::default());
            h.cursor("x")
        })
    };
    // Attached, the document's cursor: a hand over each.
    assert_eq!(shapes(&[]), [Some("pointer"); 5]);
    // Detached, a hand only over the hyperlink: text over text, the
    // default pointer elsewhere (beside the button's centred label).
    assert_eq!(
        shapes(&[("d", "1")]),
        [
            Some("default"),
            Some("text"),
            Some("text"),
            Some("default"),
            Some("pointer")
        ]
    );
}

#[test]
fn what_the_user_typed_stays_after_detach() {
    let field = |value: &str| {
        format!(
            "<style>body{{margin:0}} input{{width:200px}}</style><input id=name value='{value}'>"
        )
    };
    let mut typed = host();
    doc_at(&mut typed, "f", &[], &field(""), "2");
    typed.handle(&cmd(
        &[("a", "focus"), ("s", "f"), ("t", "name"), ("q", "2")],
        "",
    ));
    type_text(&mut typed, "yz");
    typed.handle(&cmd(&[("a", "detach"), ("s", "f"), ("q", "2")], ""));
    render(&mut typed);
    let frame = |h: &Host| h.frame("f").unwrap().rgba.clone();
    let mut written = host();
    doc_at(&mut written, "f", &[("d", "1")], &field("yz"), "2");
    let mut empty = host();
    doc_at(&mut empty, "f", &[("d", "1")], &field(""), "2");
    assert_eq!(
        frame(&typed),
        frame(&written),
        "the field shows what was typed"
    );
    assert_ne!(frame(&typed), frame(&empty));
    // The document still has the program's value.
    assert_eq!(typed.inspect("f", "name").unwrap()["attrs"]["value"], "");
}

/// Places the form again, asking for presses or not (SPEC §5.2 `p`).
fn place_form_presses(h: &mut Host, presses: bool) {
    let mut pairs = vec![("a", "place"), ("s", "f"), ("c", "30"), ("r", "5"), ("q", "2")];
    if presses {
        pairs.push(("p", "1"));
    }
    h.handle(&cmd(&pairs, ""));
    render(h);
}

#[test]
fn a_press_is_reported_wherever_it_lands_when_the_placement_asks() {
    let mut h = host();
    place_form(&mut h);
    let r = replies(&h.handle(&cmd(&[("a", "q"), ("n", "1")], "")));
    let caps: serde_json::Value = serde_json::from_slice(&r[0].payload).unwrap();
    assert!(caps["events"].as_array().unwrap().iter().any(|e| e == "press"));
    // Not asked for: a press on text says nothing (SPEC §10.1).
    assert_eq!(kinds(&click(&mut h, "f", 10.0, 30.0)), vec![]);

    place_form_presses(&mut h, true);
    // Text: the id of the element it is in. Empty space: none.
    assert_eq!(kinds(&click(&mut h, "f", 10.0, 30.0)), vec![ev("press", "count")]);
    assert_eq!(kinds(&click(&mut h, "f", 250.0, 90.0)), vec![ev("press", "")]);
    // A button: the press first, then what it does (SPEC §9).
    assert_eq!(
        kinds(&click(&mut h, "f", 10.0, 65.0)),
        vec![ev("press", "go"), ev("focus", ""), ev("click", "go")]
    );
    // Space activates the button: a click, but no press.
    let space = h.key(
        "f",
        &Key {
            name: KeyName::Space,
            mods: Mods::default(),
        },
    );
    assert_eq!(kinds(&replies(&space.effects)), vec![ev("click", "go")]);
    assert_eq!(
        kinds(&click(&mut h, "f", 10.0, 30.0)),
        vec![ev("press", "count"), ev("blur", "")]
    );
    // A new document keeps the placement, and with it `p`.
    h.handle(&cmd(&[("a", "doc"), ("s", "f"), ("q", "2")], "<p id=new style=margin:0>new</p>"));
    render(&mut h);
    assert_eq!(kinds(&click(&mut h, "f", 10.0, 10.0)), vec![ev("press", "new")]);

    // Placing again without it stops them; so does hiding.
    place_form_presses(&mut h, false);
    assert_eq!(kinds(&click(&mut h, "f", 10.0, 10.0)), vec![]);
    place_form_presses(&mut h, true);
    h.handle(&cmd(&[("a", "hide"), ("s", "f"), ("q", "2")], ""));
    assert_eq!(kinds(&click(&mut h, "f", 10.0, 10.0)), vec![]);
    // A detached surface reports nothing, whatever its placement asked.
    place_form_presses(&mut h, true);
    h.handle(&cmd(&[("a", "detach"), ("s", "f"), ("q", "2")], ""));
    assert_eq!(kinds(&click(&mut h, "f", 10.0, 10.0)), vec![]);
}

#[test]
fn a_press_comes_before_the_blur_it_causes_on_another_surface() {
    let mut h = host();
    place_form(&mut h);
    h.handle(&cmd(
        &[("a", "focus"), ("s", "f"), ("t", "name"), ("q", "2")],
        "",
    ));
    type_text(&mut h, "yz");
    doc_at(&mut h, "x", &[], "<p>text</p>", "2");
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "30"), ("r", "2"), ("p", "1"), ("q", "2")],
        "",
    ));
    let got = replies(&h.pointer("x", PointerKind::Down, 20.0, 25.0, Mods::default()));
    let order: Vec<(&str, &str)> = got
        .iter()
        .map(|c| (c.get("s").unwrap_or(""), c.get("e").unwrap_or("")))
        .collect();
    assert_eq!(order, vec![("x", "press"), ("f", "change"), ("f", "blur")]);
}

#[test]
fn a_press_with_alt_is_the_programs_and_gives_the_keyboard_back() {
    // SPEC §9.2: as a press on the cells. The surface with the keyboard
    // commits and gives it back; the surface pressed reports no `press`,
    // though its placement asked, nor anything else until the release.
    let mut h = host();
    place_form(&mut h);
    h.handle(&cmd(
        &[("a", "focus"), ("s", "f"), ("t", "name"), ("q", "2")],
        "",
    ));
    type_text(&mut h, "yz");
    doc_at(&mut h, "x", &[], "<p id=p data-on=click>text</p>", "2");
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "30"), ("r", "2"), ("p", "1"), ("q", "2")],
        "",
    ));
    let alt = Mods {
        alt: true,
        ..Mods::default()
    };
    let pairs = |fx: &[Effect]| -> Vec<(String, String)> {
        replies(fx)
            .iter()
            .map(|c| (c.get("s").unwrap_or("").into(), c.get("e").unwrap_or("").into()))
            .collect()
    };
    let got = pairs(&h.pointer("x", PointerKind::Down, 20.0, 25.0, alt));
    assert_eq!(got, vec![("f".into(), "change".into()), ("f".into(), "blur".into())]);
    assert!(!h.is_focused("f"));
    // Alt let go: the gesture is still the program's, to its release.
    assert_eq!(pairs(&h.pointer("x", PointerKind::Move, 22.0, 25.0, Mods::default())), vec![]);
    assert_eq!(pairs(&h.pointer("x", PointerKind::Up, 22.0, 25.0, Mods::default())), vec![]);
    // The next press without Alt is the surface's again.
    let got = pairs(&h.pointer("x", PointerKind::Down, 20.0, 25.0, Mods::default()));
    assert_eq!(got, vec![("x".into(), "press".into())]);
    let got = pairs(&h.pointer("x", PointerKind::Up, 20.0, 25.0, Mods::default()));
    assert_eq!(got, vec![("x".into(), "click".into())]);
}

/// A drag's events with their detail: `(e, t, detail)`.
fn drags(ev: &[Command]) -> Vec<(String, String, serde_json::Value)> {
    ev.iter()
        .filter(|c| c.get("a") == Some("ev"))
        .map(|c| {
            (
                c.get("e").unwrap_or("").to_string(),
                c.get("t").unwrap_or("").to_string(),
                serde_json::from_slice(&c.payload).unwrap_or(serde_json::Value::Null),
            )
        })
        .collect()
}

fn drag(e: &str, t: &str, c: i32, r: i32, keys: &[&str]) -> (String, String, serde_json::Value) {
    (
        e.to_string(),
        t.to_string(),
        serde_json::json!({ "c": c, "r": r, "keys": keys }),
    )
}

/// Two cells three columns wide in row 1 of a 2-row surface, and one in
/// row 0. The host's cells are 10×20 px.
const CELLS: &str = "<style>body{margin:0}i{position:absolute;width:30px;height:20px}</style>
    <i id=top data-on=drag style=left:0;top:0>T</i>
    <i id=a data-on=drag style=left:0;top:20px><b><span>A</span></b></i>
    <i id=b data-on='click drag' style=left:30px;top:20px>B</i>";

#[test]
fn a_drag_comes_after_its_press_and_before_the_blur_it_causes() {
    let mut h = host();
    let r = replies(&h.handle(&cmd(&[("a", "q"), ("n", "1")], "")));
    let caps: serde_json::Value = serde_json::from_slice(&r[0].payload).unwrap();
    assert!(
        caps["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e == "drag")
    );
    place_form(&mut h);
    h.handle(&cmd(
        &[("a", "focus"), ("s", "f"), ("t", "name"), ("q", "2")],
        "",
    ));
    type_text(&mut h, "yz");
    doc_at(&mut h, "x", &[], CELLS, "2");
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "30"),
            ("r", "2"),
            ("p", "1"),
            ("q", "2"),
        ],
        "",
    ));
    render(&mut h);
    let shift = Mods {
        shift: true,
        ..Mods::default()
    };
    let got = replies(&h.pointer("x", PointerKind::Down, 15.0, 30.0, shift));
    let order: Vec<(&str, &str, &str)> = got
        .iter()
        .map(|c| {
            (
                c.get("s").unwrap_or(""),
                c.get("e").unwrap_or(""),
                c.get("t").unwrap_or(""),
            )
        })
        .collect();
    // The press, then the drag, then what the press causes (SPEC §9.1).
    assert_eq!(
        order,
        vec![
            ("x", "press", "a"),
            ("x", "dragstart", "a"),
            ("f", "change", ""),
            ("f", "blur", "")
        ]
        .into_iter()
        .map(|(s, e, t)| (s, e, if e == "change" { "name" } else { t }))
        .collect::<Vec<_>>()
    );
    assert_eq!(
        drags(&got[1..2]),
        vec![drag("dragstart", "a", 1, 1, &["shift"])]
    );
}

#[test]
fn a_drag_has_no_target_outside_the_window_and_ends_when_the_pointer_is_lost() {
    let mut h = host();
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], CELLS));
    // The window shows row 1 only: `top`, in row 0, is laid out, not shown.
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "30"),
            ("r", "2"),
            ("y", "1"),
            ("q", "2"),
        ],
        "",
    ));
    render(&mut h);
    let m = Mods::default();
    let mut got = h.pointer("x", PointerKind::Down, 45.0, 30.0, m);
    got.extend(h.pointer("x", PointerKind::Move, 15.0, 10.0, m));
    got.extend(h.pointer("x", PointerKind::Move, 15.0, 30.0, m));
    // Held: far outside, then left of and above the surface.
    got.extend(h.pointer("x", PointerKind::Move, 512.0, 75.0, m));
    got.extend(h.pointer("x", PointerKind::Move, -5.0, -25.0, m));
    // The host lost the pointer: the drag ends where it last was.
    got.extend(h.pointer("x", PointerKind::Leave, 0.0, 0.0, m));
    got.extend(h.pointer("x", PointerKind::Up, 15.0, 30.0, m));
    assert_eq!(
        drags(&replies(&got)),
        vec![
            drag("dragstart", "b", 4, 1, &[]),
            drag("drag", "", 1, 0, &[]),
            drag("drag", "a", 1, 1, &[]),
            drag("drag", "", 51, 3, &[]),
            drag("drag", "", -1, -2, &[]),
            drag("dragend", "", -1, -2, &[]),
        ]
    );
}

#[test]
fn a_drag_selects_no_text_and_neither_does_user_select_none() {
    // Frames before and after a press and a drag across the text: a
    // selection would paint its highlight.
    let selects = |html: &str, cells: bool| {
        let mut h = host();
        h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], html));
        h.handle(&cmd(
            &[
                ("a", "place"),
                ("s", "x"),
                ("c", "30"),
                ("r", "8"),
                ("q", "2"),
            ],
            "",
        ));
        render(&mut h);
        let before = h.frame("x").unwrap().rgba.clone();
        let m = Mods::default();
        let mut got = h.pointer("x", PointerKind::Down, 5.0, 32.0, m);
        for (x, y) in [(60.0, 32.0), (200.0, 50.0), (120.0, 75.0)] {
            got.extend(h.pointer("x", PointerKind::Move, x, y, m));
        }
        render(&mut h);
        let changed = h.frame("x").unwrap().rgba != before;
        got.extend(h.pointer("x", PointerKind::Up, 120.0, 75.0, m));
        assert_eq!(!drags(&replies(&got)).is_empty(), cells, "{html}");
        changed
    };
    assert!(selects(PROSE, false), "plain prose is selected");
    let none = PROSE
        .replace(
            "<p>The first",
            "<div style=user-select:none><p><b>The first",
        )
        .replace(
            "third, below the rest.</p>",
            "third, below the rest.</b></p></div>",
        );
    assert!(
        !selects(&none, false),
        "user-select: none, three levels up, selects nothing"
    );
    let opted = PROSE
        .replace("<p>The first", "<div id=d data-on=drag><p><b>The first")
        .replace(
            "third, below the rest.</p>",
            "third, below the rest.</b></p></div>",
        );
    assert!(
        !selects(&opted, true),
        "an element that opts in to drags selects nothing"
    );
}

/// The capabilities name the implementation and its version (SPEC §4, §15):
/// a program patches a surface's vars only from the version that lays a
/// var patch out like a fresh document (0.0.2).
#[test]
fn the_capabilities_name_the_host_and_its_version() {
    let mut h = host();
    let r = replies(&h.handle(&cmd(&[("a", "q")], "")));
    let caps = serde_json::from_slice::<serde_json::Value>(&r[0].payload).unwrap();
    assert_eq!(caps["host"], "hotty-blitz");
    assert_eq!(caps["version"], env!("CARGO_PKG_VERSION"));
    let v: Vec<u32> = env!("CARGO_PKG_VERSION").split('.').map(|n| n.parse().unwrap()).collect();
    assert!(v >= vec![0, 0, 2], "a_var_patch_lays_out_like_a_fresh_document's fix is 0.0.2");
}

/// The capabilities say `passthrough` only when the terminal hands the
/// pointer through (SPEC §4, §9.3), and the surface says where it takes it.
#[test]
fn passthrough_is_the_terminals_to_announce_and_pointer_events_decide_where() {
    let mut h = host();
    let caps = |h: &mut Host| {
        let r = replies(&h.handle(&cmd(&[("a", "q")], "")));
        serde_json::from_slice::<serde_json::Value>(&r[0].payload).unwrap()
    };
    assert!(caps(&mut h).get("passthrough").is_none());
    h.set_passthrough(true);
    assert_eq!(caps(&mut h)["passthrough"], true);
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x"), ("q", "2")],
        "<style>html{pointer-events:none}body{margin:0}b{position:absolute;left:0;top:0;width:10px;height:20px;pointer-events:auto}</style><b></b>",
    ));
    h.handle(&cmd(&[("a", "place"), ("s", "x"), ("c", "5"), ("r", "1"), ("q", "2")], ""));
    render(&mut h);
    assert!(h.takes_pointer("x", 5.0, 10.0), "the box with pointer-events: auto takes it");
    assert!(!h.takes_pointer("x", 30.0, 10.0), "the rest lets it through");
    assert!(!h.takes_pointer("nope", 5.0, 10.0), "no such surface takes nothing");
}

/// A custom property that changes a containing block's height and its
/// transform must leave its stretched absolute child painted as a fresh
/// document paints it, here at a fractional scale (15x33 px cells, 1.604).
/// The child's size changes with no style damage of its own; Blitz kept its
/// old overflow, paint culled it by that, and nothing of it was painted
/// (blitz/README.md, "Overflow after a relayout").
#[test]
fn a_var_patch_lays_out_like_a_fresh_document() {
    let html = |h: u32| {
        format!(
            "<html id=root style='--h:{h}'><style>html,body{{margin:0;background:transparent}}
        .ring{{position:absolute;left:0;top:0;width:300px;height:calc(var(--h)*20px);transform:translateY(calc((1 - var(--h))*20px))}}
        .line{{position:absolute;left:4px;right:4px;top:9px;bottom:9px;border:2px solid #f00}}</style>
        <div class=ring><div class=line></div></div></html>"
        )
    };
    let painted = |h: &Host| h.frame("x").unwrap().rgba.chunks(4).filter(|p| p[3] > 0).count();
    let host = || {
        Host::new(Config {
            metrics: Metrics { cell_w: 15, cell_h: 33, scale: 1.604167 },
            ..Config::default()
        })
    };
    let mut inc = host();
    inc.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], &html(5)));
    inc.handle(&cmd(&[("a", "place"), ("s", "x"), ("c", "30"), ("r", "1"), ("q", "2")], ""));
    render(&mut inc);
    inc.handle(&cmd(
        &[("a", "patch"), ("s", "x"), ("op", "var"), ("t", "root"), ("k", "h"), ("q", "2")],
        "6",
    ));
    render(&mut inc);
    let mut fresh = host();
    fresh.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], &html(6)));
    fresh.handle(&cmd(&[("a", "place"), ("s", "x"), ("c", "30"), ("r", "1"), ("q", "2")], ""));
    render(&mut fresh);
    assert!(painted(&fresh) > 0, "the fresh document paints the line");
    assert_eq!(painted(&inc), painted(&fresh));
}

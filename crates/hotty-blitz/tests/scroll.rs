//! Documents that scroll (SPEC §5.1, §5.3): what the shared vectors do not
//! reach. Painting, scrollbars, the axes a document did not ask for, and the
//! keys a control keeps.

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

fn render(h: &mut Host) -> Vec<hotty_blitz::Damage> {
    let mut out = Vec::new();
    h.render_dirty(&mut |_, _, d| out.push(d.clone()));
    out
}

/// A document with `scroll` (or none), placed `c`×`r` with presses
/// reported, and drawn.
fn place(h: &mut Host, scroll: Option<&str>, html: &str, c: &str, r: &str) {
    let mut pairs = vec![("a", "doc"), ("s", "x"), ("q", "2")];
    if let Some(v) = scroll {
        pairs.push(("scroll", v));
    }
    h.handle(&cmd(&pairs, html));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", c),
            ("r", r),
            ("p", "1"),
            ("q", "2"),
        ],
        "",
    ));
    render(h);
}

/// The detail of the press at device pixel (`x`, `y`): its target and area.
fn press(h: &mut Host, x: f32, y: f32) -> (String, serde_json::Value) {
    let mut fx = h.pointer("x", PointerKind::Down, x, y, Mods::default());
    fx.extend(h.pointer("x", PointerKind::Up, x, y, Mods::default()));
    let p = replies(&fx)
        .into_iter()
        .find(|c| c.get("e") == Some("press"))
        .expect("a press");
    let detail = serde_json::from_slice(&p.payload).unwrap_or(serde_json::Value::Null);
    (p.get("t").unwrap_or("").to_string(), detail)
}

fn wheel(h: &mut Host, x: f32, y: f32, dx: f32, dy: f32) -> bool {
    let taken = h.wheel("x", x, y, dx, dy, Mods::default()).taken;
    h.end_gesture();
    taken
}

fn key(h: &mut Host, name: KeyName) -> bool {
    h.key(
        "x",
        &Key {
            name,
            mods: Mods::default(),
        },
    )
    .consumed
}

/// The pixels where the last frame and a full paint of the same state
/// differ.
fn differs_from_a_full_paint(h: &mut Host) -> usize {
    let before = h.frame("x").unwrap().rgba.clone();
    h.repaint();
    render(h);
    let after = &h.frame("x").unwrap().rgba;
    before
        .chunks(4)
        .zip(after.chunks(4))
        .filter(|(p, q)| p.iter().zip(q.iter()).any(|(a, b)| a.abs_diff(*b) > 1))
        .count()
}

const ROWS: &str = "<style>body{margin:0}div{height:20px}</style>\
    <div id=a>a</div><div id=b>b</div><div id=c>c</div><div id=d>d</div>\
    <div id=e>e</div><div id=f>f</div><div id=g>g</div><div id=h>h</div>";

const BOX: &str = "<style>body{margin:0}div{height:20px;width:40px}\
    #box{height:60px;width:100px;overflow:auto;background:#123}\
    #box div{background:#4a6;margin-bottom:0}</style>\
    <div id=top>top</div><div id=box><div id=b0>b0</div><div id=b1>b1</div>\
    <div id=b2>b2</div><div id=b3>b3</div><div id=b4>b4</div><div id=b5>b5</div>\
    <div id=b6>b6</div><div id=b7>b7</div></div><div id=after>after</div>";

#[test]
fn an_element_scrolled_repaints_in_part_as_a_full_paint_draws_it() {
    let mut h = host();
    place(&mut h, Some("1"), BOX, "20", "4");
    assert!(wheel(&mut h, 15.0, 30.0, 0.0, 30.0));
    let damage = render(&mut h);
    assert!(
        matches!(damage.as_slice(), [hotty_blitz::Damage::Rects(_)]),
        "an element's scroll repaints the element: {damage:?}"
    );
    assert_eq!(differs_from_a_full_paint(&mut h), 0);
}

#[test]
fn a_change_after_the_root_scrolled_repaints_in_part_as_a_full_paint_draws_it() {
    let mut h = host();
    place(&mut h, Some("1"), ROWS, "20", "4");
    assert!(wheel(&mut h, 15.0, 10.0, 0.0, 50.0));
    render(&mut h);
    h.handle(&cmd(
        &[
            ("a", "delta"),
            ("s", "x"),
            ("op", "text"),
            ("t", "d"),
            ("q", "2"),
        ],
        "d, longer now",
    ));
    let damage = render(&mut h);
    assert!(
        matches!(damage.as_slice(), [hotty_blitz::Damage::Rects(_)]),
        "{damage:?}"
    );
    assert_eq!(differs_from_a_full_paint(&mut h), 0);
}

#[test]
fn the_roots_scrollbar_shows_as_it_scrolls_and_fades() {
    let mut h = host();
    place(&mut h, Some("1"), ROWS, "20", "4");
    // The thumb's track: 2 px from the right edge, 10 px wide.
    let strip = |h: &Host| -> Vec<u8> {
        let f = h.frame("x").unwrap();
        (0..f.height)
            .flat_map(|y| {
                let i = ((y * f.width + f.width - 7) * 4) as usize;
                f.rgba[i..i + 3].to_vec()
            })
            .collect()
    };
    let rest = strip(&h);
    assert!(wheel(&mut h, 15.0, 10.0, 0.0, 30.0));
    render(&mut h);
    assert_ne!(strip(&h), rest, "a thumb shows while the root scrolls");
    // Drawn as a terminal draws it: one timer, set to next_frame, and a
    // render when it fires.
    let start = std::time::Instant::now();
    let mut frames = 0;
    while let Some(due) = h.next_frame() {
        assert!(start.elapsed().as_millis() < 2000, "the fade ends");
        std::thread::sleep(due.saturating_duration_since(std::time::Instant::now()));
        assert!(
            h.has_dirty(),
            "a frame of the fade is due when its timer fires"
        );
        render(&mut h);
        frames += 1;
    }
    assert!(frames > 1, "the thumb fades over frames, not at once");
    // The rows have nothing at the right edge: once faded, the strip is
    // the background again.
    assert_eq!(strip(&h), rest, "the thumb faded");
    assert_eq!(differs_from_a_full_paint(&mut h), 0);
}

#[test]
fn a_thumb_drags_its_element_and_its_press_is_not_the_documents() {
    let mut h = host();
    place(&mut h, Some("1"), BOX, "20", "4");
    // To the end: the thumb shows, at the bottom of the box (rows 1 to 3,
    // 60 px; 160 px of content, so a 32 px thumb from y = 48).
    assert!(wheel(&mut h, 15.0, 30.0, 0.0, 200.0));
    render(&mut h);
    let mut fx = h.pointer("x", PointerKind::Down, 93.0, 66.0, Mods::default());
    fx.extend(h.pointer("x", PointerKind::Move, 93.0, 30.0, Mods::default()));
    fx.extend(h.pointer("x", PointerKind::Up, 93.0, 30.0, Mods::default()));
    assert_eq!(
        replies(&fx).len(),
        0,
        "the document hears nothing of a thumb"
    );
    render(&mut h);
    // Dragged back to the top.
    let (t, _) = press(&mut h, 15.0, 30.0);
    assert_eq!(t, "b0");
}

#[test]
fn the_roots_thumb_drags_the_page() {
    let mut h = host();
    place(&mut h, Some("1"), ROWS, "20", "4");
    assert!(wheel(&mut h, 15.0, 10.0, 0.0, 10.0));
    render(&mut h);
    // 80 px tall, 160 px of content: a 40 px thumb, 5 px down after 10 px.
    let mut fx = h.pointer("x", PointerKind::Down, 193.0, 20.0, Mods::default());
    fx.extend(h.pointer("x", PointerKind::Move, 193.0, 80.0, Mods::default()));
    fx.extend(h.pointer("x", PointerKind::Up, 193.0, 80.0, Mods::default()));
    assert_eq!(
        replies(&fx).len(),
        0,
        "the document hears nothing of a thumb"
    );
    render(&mut h);
    let (t, _) = press(&mut h, 15.0, 10.0);
    assert_eq!(t, "e", "dragged to the end");
}

#[test]
fn along_an_axis_not_asked_for_nothing_moves() {
    let mut h = host();
    // Asked to scroll down only; a button far to the right, and a link to
    // an element far to the right and below.
    place(
        &mut h,
        Some("1"),
        "<style>body{margin:0;width:600px}div{height:20px}\
         button,#far{position:relative;left:400px;display:block;margin:0;padding:0;border:0;height:20px;width:40px}</style>\
         <a id=go href=#far>go</a><div></div><div></div><div></div><div></div>\
         <button id=k>k</button><div></div><div></div><b id=far>far</b>",
        "20",
        "4",
    );
    assert!(
        !wheel(&mut h, 15.0, 10.0, 200.0, 0.0),
        "across: the terminal's"
    );
    h.handle(&cmd(
        &[("a", "focus"), ("s", "x"), ("t", "k"), ("q", "2")],
        "",
    ));
    render(&mut h);
    let fx = h.key(
        "x",
        &Key {
            name: KeyName::Enter,
            mods: Mods::default(),
        },
    );
    let click = replies(&fx.effects)
        .into_iter()
        .find(|c| c.get("e") == Some("click"))
        .expect("a click");
    let detail: serde_json::Value = serde_json::from_slice(&click.payload).unwrap();
    // Brought into view down (row 5 of 4, so scrolled 2 rows), never across.
    assert_eq!(
        detail["area"],
        serde_json::json!({"c": 40, "r": 3, "w": 4, "h": 1})
    );
}

#[test]
fn a_document_that_did_not_ask_takes_no_wheel() {
    let mut h = host();
    place(&mut h, None, ROWS, "20", "4");
    assert!(!wheel(&mut h, 15.0, 10.0, 0.0, 50.0));
    let (t, _) = press(&mut h, 15.0, 10.0);
    assert_eq!(t, "a");
}

#[test]
fn a_gesture_begun_over_the_cells_stays_the_terminals() {
    let mut h = host();
    place(&mut h, Some("1"), ROWS, "20", "4");
    let over_cells = h.wheel("", 0.0, 0.0, 0.0, 20.0, Mods::default());
    assert!(!over_cells.taken);
    // The same gesture reaches the surface: still the terminal's.
    assert!(!h.wheel("x", 15.0, 10.0, 0.0, 20.0, Mods::default()).taken);
    h.end_gesture();
    // A new one scrolls it.
    assert!(h.wheel("x", 15.0, 10.0, 0.0, 20.0, Mods::default()).taken);
}

#[test]
fn shift_turns_a_wheel_across() {
    let mut h = host();
    place(
        &mut h,
        Some("2"),
        "<style>body{margin:0;width:600px;height:20px}i{position:absolute;top:0;display:block;width:20px;height:20px}</style>\
         <i id=l style=left:0>l</i><i id=r style=left:500px>r</i>",
        "20",
        "4",
    );
    let shift = Mods {
        shift: true,
        ..Mods::default()
    };
    assert!(h.wheel("x", 15.0, 10.0, 0.0, 1000.0, shift).taken);
    h.end_gesture();
    render(&mut h);
    let (t, _) = press(&mut h, 105.0, 10.0);
    assert_eq!(t, "r", "scrolled 400 px across");
}

#[test]
fn focus_on_what_covers_the_port_leaves_it_where_it_is() {
    // A page that focuses its root so that the keys scroll it (hotty-demo)
    // does not jump back to its top when it focuses it again: an element
    // that covers the port is in view (CSSOM View's "nearest").
    let mut h = host();
    let page = format!("<main id=root tabindex=-1>{ROWS}</main>");
    place(&mut h, Some("1"), &page, "20", "4");
    assert!(wheel(&mut h, 15.0, 10.0, 0.0, 60.0));
    render(&mut h);
    assert_eq!(top(&h), "d");
    h.handle(&cmd(
        &[("a", "focus"), ("s", "x"), ("t", "root"), ("q", "2")],
        "",
    ));
    render(&mut h);
    assert_eq!(top(&h), "d", "the focus moved nothing");
}

#[test]
fn a_textarea_keeps_the_keys_it_types_with() {
    let mut h = host();
    place(
        &mut h,
        Some("1"),
        &format!("{ROWS}<textarea id=t></textarea>"),
        "20",
        "4",
    );
    h.handle(&cmd(
        &[("a", "focus"), ("s", "x"), ("t", "t"), ("q", "2")],
        "",
    ));
    render(&mut h);
    let before = top(&h);
    for k in [
        KeyName::Space,
        KeyName::Home,
        KeyName::End,
        KeyName::PageUp,
        KeyName::Down,
    ] {
        assert!(key(&mut h, k.clone()), "{k:?} is the textarea's");
    }
    render(&mut h);
    assert_eq!(top(&h), before, "the keys scrolled nothing");
}

/// The row at the top of the surface, read without a press (which would
/// take the keyboard).
fn top(h: &Host) -> String {
    ["a", "b", "c", "d", "e", "f", "g", "h"]
        .into_iter()
        .find(|id| {
            h.element_centre("x", id)
                .is_some_and(|(_, y)| (y - 10.0).abs() < 1.0)
        })
        .unwrap_or_default()
        .to_string()
}

fn ch(c: &str) -> KeyName {
    KeyName::Char(c.into())
}

/// The events a key made, as (`e`, `t`).
fn key_events(h: &mut Host, name: KeyName) -> (bool, Vec<(String, String)>) {
    let out = h.key(
        "x",
        &Key {
            name,
            mods: Mods::default(),
        },
    );
    let events = replies(&out.effects)
        .into_iter()
        .filter(|c| c.get("a") == Some("ev"))
        .map(|c| {
            (
                c.get("e").unwrap_or("").to_string(),
                c.get("t").unwrap_or("").to_string(),
            )
        })
        .collect();
    (out.consumed, events)
}

/// Where an element's centre is, in device pixels from the surface's top.
fn y(h: &Host, id: &str) -> f32 {
    h.element_centre("x", id).expect("the element").1
}

#[test]
fn scroll_actions_scroll_the_nearest_box_then_outward_never_the_terminal() {
    // #box is 60px high and holds 160px: it moves 100px. The page, 100px
    // high in 80px, moves 20px.
    let mut h = host();
    let page = BOX.replace(
        "<div id=box>",
        "<div id=box tabindex=0 data-keys=\"j=scroll-down d=scroll-half-page-down \
         g=scroll-start G=scroll-end\">",
    );
    place(&mut h, Some("1"), &page, "20", "4");
    h.handle(&cmd(
        &[("a", "focus"), ("s", "x"), ("t", "box"), ("q", "2")],
        "",
    ));
    render(&mut h);
    assert_eq!(y(&h, "b0"), 30.0);
    // An arrow's step, then half the port.
    assert!(key(&mut h, ch("j")));
    render(&mut h);
    assert_eq!(y(&h, "b0"), -10.0);
    assert!(key(&mut h, ch("d")));
    render(&mut h);
    assert_eq!(y(&h, "b0"), -40.0);
    // To the box's end; then, the box at its end, the page's.
    assert!(key(&mut h, ch("G")));
    render(&mut h);
    assert_eq!(y(&h, "b0"), -70.0);
    assert_eq!(y(&h, "top"), 10.0);
    assert!(key(&mut h, ch("G")));
    render(&mut h);
    assert_eq!(y(&h, "top"), -10.0);
    // Nothing can move down: the key is used, and the terminal moves not.
    assert!(key(&mut h, ch("j")));
    // Up, the box comes first again.
    assert!(key(&mut h, ch("g")));
    render(&mut h);
    assert_eq!(y(&h, "b0"), 10.0);
    assert_eq!(y(&h, "top"), -10.0);
    // A key bound to nothing still reaches the program.
    assert!(!key(&mut h, ch("x")));
}

#[test]
fn a_scroll_binding_leaves_the_elements_own_keys_and_its_fields_alone() {
    let mut h = host();
    let page = format!(
        "<div id=box style=\"height:40px;overflow:auto\" \
         data-keys=\"Space=scroll-page-down j=scroll-down k=program ArrowDown=program\">\
         <button id=b>b</button><input id=i data-on=input data-keys=\"k=scroll-up \
         ArrowDown=scroll-down\">{ROWS}</div>"
    );
    place(&mut h, Some("1"), &page, "20", "4");
    let at = y(&h, "a");
    h.handle(&cmd(
        &[("a", "focus"), ("s", "x"), ("t", "b"), ("q", "2")],
        "",
    ));
    // Space presses the button; j, which a button does not use, scrolls.
    assert_eq!(
        key_events(&mut h, KeyName::Space),
        (true, vec![("click".to_string(), "b".to_string())])
    );
    render(&mut h);
    assert_eq!(y(&h, "a"), at, "Space scrolled nothing");
    h.handle(&cmd(
        &[("a", "focus"), ("s", "x"), ("t", "i"), ("q", "2")],
        "",
    ));
    render(&mut h);
    let at = y(&h, "a");
    // In the field, j is typed, and a nearer scroll binding of k or of the
    // arrow does not hide the farther program.
    assert_eq!(
        key_events(&mut h, ch("j")),
        (true, vec![("input".to_string(), "i".to_string())])
    );
    assert_eq!(key_events(&mut h, ch("k")), (false, vec![]));
    assert_eq!(key_events(&mut h, KeyName::Down), (false, vec![]));
    render(&mut h);
    assert_eq!(y(&h, "a"), at, "nothing scrolled");
}

#[test]
fn along_an_axis_not_asked_for_a_scroll_binding_goes_on_as_if_unbound() {
    let mut h = host();
    let page = format!(
        "<main id=m tabindex=0 data-keys=\"l=scroll-right j=scroll-down\" \
         style=\"width:400px\">{ROWS}</main>"
    );
    place(&mut h, Some("1"), &page, "20", "4");
    h.handle(&cmd(
        &[("a", "focus"), ("s", "x"), ("t", "m"), ("q", "2")],
        "",
    ));
    assert!(!key(&mut h, ch("l")), "the page does not scroll across");
    assert!(key(&mut h, ch("j")));
    // A page that asked for nothing: every binding goes on to the program.
    place(&mut h, None, &page, "20", "4");
    h.handle(&cmd(
        &[("a", "focus"), ("s", "x"), ("t", "m"), ("q", "2")],
        "",
    ));
    assert!(!key(&mut h, ch("j")));
}

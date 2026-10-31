//! CSS Scroll Snap: a box or a root that asks with `scroll-snap-type` comes
//! to rest on a snap position after every scroll: a key's line or page, the
//! wheel, a thumb let go, `reveal`. Blitz picks the position
//! (mods/blitz/scroll-snap.md) from properties Stylo computes in servo mode
//! (mods/stylo/scroll-snap.md); a document without them scrolls as it did.

mod common;

use common::delta_cmd;
use hotty::Message;
use hotty_blitz::{Config, Damage, Host, Key, KeyName, Metrics, Mods, PointerKind};
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

fn render(h: &mut Host) -> Vec<Damage> {
    let mut out = Vec::new();
    h.render_dirty(&mut |_, _, d| out.push(d.clone()));
    out
}

/// A document that scrolls down, placed `c`×`r` with presses reported, and
/// drawn.
fn place(h: &mut Host, html: &str, c: &str, r: &str) {
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x"), ("q", "2"), ("scroll", "1")],
        html,
    ));
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

fn focus(h: &mut Host, id: &str) {
    h.handle(&cmd(
        &[("a", "focus"), ("s", "x"), ("t", id), ("q", "2")],
        "",
    ));
    render(h);
}

fn key(h: &mut Host, name: KeyName) -> bool {
    let consumed = h
        .key(
            "x",
            &Key {
                name,
                names: Vec::new(),
                mods: Mods::default(),
            },
        )
        .consumed;
    render(h);
    consumed
}

/// A wheel gesture that comes to rest: its end, then a frame.
fn wheel(h: &mut Host, x: f32, y: f32, dy: f32) -> bool {
    let taken = h.wheel("x", x, y, 0.0, dy, Mods::default()).taken;
    h.end_gesture();
    render(h);
    taken
}

fn reveal(h: &mut Host, t: &str, v: &str) {
    h.handle(&delta_cmd(
        &[("s", "x"), ("q", "2")],
        "reveal",
        Some(t),
        None,
        v,
    ));
    render(h);
}

/// How long the scrollbars a scroll shows stay and then take to fade: 500
/// and 200 ms, Blitz's for an element's and the host's for the root's.
const FADE: Duration = Duration::from_millis(700);

/// The pixels where the frames drawn since the last scroll, its scrollbars'
/// fade included, and a full paint of the same state differ. It waits for
/// the fade to end, so that a full paint draws the thumbs as the frames
/// did, however long a frame takes: in a debug build under load, one can
/// take more than the 500 ms they show for at full opacity.
fn differs_from_a_full_paint(h: &mut Host) -> usize {
    std::thread::sleep(FADE + Duration::from_millis(50));
    render(h);
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

const MANDATORY: &str = "scroll-snap-type:y mandatory";
const PROXIMITY: &str = "scroll-snap-type:y proximity";
const START: &str = "scroll-snap-align:start";

/// `n` rows `row` px tall, `r0` first, in a box `#list` `height` px tall at
/// the top of the page that scrolls and takes focus, with `list` on the box
/// and `rows` on each row.
fn list(row: f32, height: f32, n: usize, list: &str, rows: &str) -> String {
    let items: String = (0..n).map(|i| format!("<div id=r{i}>{i}</div>")).collect();
    format!(
        "<style>body{{margin:0}}\
         #list{{height:{height}px;width:150px;overflow:auto;background:#123;{list}}}\
         #list div{{height:{row}px;background:#4a6;border-bottom:1px solid #fff;\
         box-sizing:border-box;{rows}}}</style>\
         <div id=list tabindex=0>{items}</div>"
    )
}

/// How far the box (or the root) has scrolled, from where its first row
/// `row` px tall is drawn.
fn offset(h: &Host, row: f32) -> f32 {
    row / 2.0 - h.element_centre("x", "r0").expect("the first row").1
}

/// A box 30.5 rows of 20 px tall, its rows aligned at their starts, and
/// focused: the keys scroll it. With `snap` on the box.
fn rows_of_20(h: &mut Host, snap: &str) {
    place(h, &list(20.0, 610.0, 100, snap, START), "20", "40");
    focus(h, "list");
}

#[test]
fn a_page_down_shows_a_whole_row_at_the_top() {
    // 20 px rows under `y mandatory`, in a box of 30.5 rows: a page is 7/8
    // of the box, 533.75 px, and it comes to rest at the row at or short of
    // that, so that it passes over nothing and shows a whole row at the top.
    let mut h = host();
    rows_of_20(&mut h, MANDATORY);
    assert!(key(&mut h, KeyName::PageDown));
    assert_eq!(offset(&h, 20.0), 520.0, "row 26 at the top");
    assert!(key(&mut h, KeyName::Space));
    assert_eq!(offset(&h, 20.0), 1040.0, "Space pages as Page Down does");
    assert!(key(&mut h, KeyName::PageUp));
    assert_eq!(offset(&h, 20.0), 520.0);
    assert!(key(&mut h, KeyName::PageUp));
    assert_eq!(offset(&h, 20.0), 0.0);
}

#[test]
fn an_arrow_rests_on_the_row_nearest_its_step() {
    // 42 px rows: an arrow's 40 px rests on the next row, not 2 px short.
    let mut h = host();
    place(&mut h, &list(42.0, 610.0, 40, MANDATORY, START), "20", "40");
    focus(&mut h, "list");
    assert!(key(&mut h, KeyName::Down));
    assert_eq!(offset(&h, 42.0), 42.0);
    assert!(key(&mut h, KeyName::Down));
    assert_eq!(offset(&h, 42.0), 84.0);
    assert!(key(&mut h, KeyName::Up));
    assert_eq!(offset(&h, 42.0), 42.0);
    // Home and End: to the ends, which are snap positions too (the last
    // rows' are as far as the box goes).
    assert!(key(&mut h, KeyName::End));
    assert_eq!(offset(&h, 42.0), 40.0 * 42.0 - 610.0);
    assert!(key(&mut h, KeyName::Home));
    assert_eq!(offset(&h, 42.0), 0.0);
}

#[test]
fn a_wheel_gesture_rests_on_a_row_when_it_ends() {
    let mut h = host();
    rows_of_20(&mut h, MANDATORY);
    // Under way it scrolls as far as the wheel says.
    assert!(h.wheel("x", 15.0, 30.0, 0.0, 45.0, Mods::default()).taken);
    assert_eq!(offset(&h, 20.0), 45.0);
    // At rest: past where it began, the row nearest where it ended.
    h.end_gesture();
    let damage = render(&mut h);
    assert_eq!(offset(&h, 20.0), 40.0);
    assert!(
        matches!(damage.as_slice(), [Damage::Rects(_)]),
        "the snap repaints the box: {damage:?}"
    );
    assert_eq!(differs_from_a_full_paint(&mut h), 0);
    // Back up a little: never where it began, nor behind it.
    assert!(wheel(&mut h, 15.0, 30.0, -3.0));
    assert_eq!(offset(&h, 20.0), 20.0);
}

#[test]
fn a_wheel_gesture_rests_when_the_terminals_timer_finds_it_ended() {
    let mut h = host();
    rows_of_20(&mut h, MANDATORY);
    let began = std::time::Instant::now();
    assert!(h.wheel("x", 15.0, 30.0, 0.0, 45.0, Mods::default()).taken);
    // A terminal sets one timer, to next_frame, and renders when it fires:
    // the rest is due once the gesture has gone 150 ms without a wheel.
    let due = h.next_frame().expect("the rest is due");
    assert!(due >= began + std::time::Duration::from_millis(150));
    std::thread::sleep(due.saturating_duration_since(std::time::Instant::now()));
    assert!(h.has_dirty(), "the rest is due when the timer fires");
    render(&mut h);
    assert_eq!(offset(&h, 20.0), 40.0);
}

#[test]
fn reveal_rests_on_a_row_that_keeps_its_element_in_view() {
    let mut h = host();
    rows_of_20(&mut h, MANDATORY);
    // Row 30 (600 to 620 px) at the box's bottom is 10 px down, between two
    // rows: the one that shows all of it.
    reveal(&mut h, "r30", "");
    assert_eq!(offset(&h, 20.0), 20.0);
    reveal(&mut h, "r60", "start");
    assert_eq!(offset(&h, 20.0), 1200.0);
    reveal(&mut h, "r60", "end");
    assert_eq!(offset(&h, 20.0), 620.0);
}

#[test]
fn a_paged_snap_repaints_in_part_as_a_full_paint_draws_it() {
    let mut h = host();
    rows_of_20(&mut h, MANDATORY);
    h.key(
        "x",
        &Key {
            name: KeyName::PageDown,
            names: Vec::new(),
            mods: Mods::default(),
        },
    );
    let damage = render(&mut h);
    assert!(
        matches!(damage.as_slice(), [Damage::Rects(_)]),
        "the page repaints the box: {damage:?}"
    );
    assert_eq!(offset(&h, 20.0), 520.0);
    assert_eq!(differs_from_a_full_paint(&mut h), 0);
}

#[test]
fn proximity_snaps_only_near_a_snap_position() {
    // Near one, as mandatory does.
    let mut h = host();
    place(&mut h, &list(42.0, 610.0, 40, PROXIMITY, START), "20", "40");
    focus(&mut h, "list");
    assert!(key(&mut h, KeyName::Down));
    assert_eq!(offset(&h, 42.0), 42.0);
    // Two rows 980 px apart, and 2000 px after the second, so that the box
    // scrolls past it: a page ends 446 px short of the second, further than
    // a third of the box. Proximity leaves it there; mandatory takes it on
    // to the row.
    let apart = |snap| {
        list(20.0, 610.0, 2, snap, START).replace(
            "<div id=r1>1</div>",
            "<p style='margin:0;height:960px'></p><div id=r1>1</div>\
             <p style='margin:0;height:2000px'></p>",
        )
    };
    let mut h = host();
    place(&mut h, &apart(PROXIMITY), "20", "40");
    focus(&mut h, "list");
    assert!(key(&mut h, KeyName::PageDown));
    assert_eq!(offset(&h, 20.0), 533.75);
    let mut h = host();
    place(&mut h, &apart(MANDATORY), "20", "40");
    focus(&mut h, "list");
    assert!(key(&mut h, KeyName::PageDown));
    assert_eq!(offset(&h, 20.0), 980.0);
}

#[test]
fn a_section_taller_than_the_box_can_be_read_to_its_end() {
    // Sections of 1000 px in a box of 610: each snaps wherever it covers the
    // box, so a page shows the first's end before the second's start, and an
    // arrow moves inside one as without snapping.
    let mut h = host();
    place(
        &mut h,
        &list(1000.0, 610.0, 3, MANDATORY, START),
        "20",
        "40",
    );
    focus(&mut h, "list");
    assert!(key(&mut h, KeyName::PageDown));
    assert_eq!(offset(&h, 1000.0), 390.0);
    assert!(key(&mut h, KeyName::PageDown));
    assert_eq!(offset(&h, 1000.0), 1000.0);
    assert!(key(&mut h, KeyName::Down));
    assert_eq!(offset(&h, 1000.0), 1040.0);
    assert!(key(&mut h, KeyName::Up));
    assert!(key(&mut h, KeyName::Up));
    assert_eq!(offset(&h, 1000.0), 390.0);
}

#[test]
fn scroll_padding_insets_the_snapport() {
    // 10 px of padding at the top: a row comes to rest 10 px down the box.
    let mut h = host();
    let padded = format!("{MANDATORY};scroll-padding:10px 0 0");
    rows_of_20(&mut h, &padded);
    assert!(key(&mut h, KeyName::PageDown));
    assert_eq!(offset(&h, 20.0), 530.0, "row 27 below the padding");
    // `reveal start` brings row 40 below the padding, not under it.
    reveal(&mut h, "r40", "start");
    assert_eq!(offset(&h, 20.0), 790.0);
    // Logical and percentage paddings: 10% of the box is 61 px.
    let mut h = host();
    rows_of_20(
        &mut h,
        &format!("{MANDATORY};scroll-padding-block-start:10%"),
    );
    assert!(key(&mut h, KeyName::PageDown));
    assert_eq!(offset(&h, 20.0), 519.0);
}

#[test]
fn end_and_center_align_a_row_with_the_boxs_bottom_and_middle() {
    let mut h = host();
    place(
        &mut h,
        &list(20.0, 610.0, 100, MANDATORY, "scroll-snap-align:end"),
        "20",
        "40",
    );
    focus(&mut h, "list");
    assert!(key(&mut h, KeyName::PageDown));
    // Row 56's bottom (1140 px) at the box's.
    assert_eq!(offset(&h, 20.0), 530.0);
    let mut h = host();
    place(
        &mut h,
        &list(20.0, 610.0, 100, MANDATORY, "scroll-snap-align:center"),
        "20",
        "40",
    );
    focus(&mut h, "list");
    assert!(key(&mut h, KeyName::PageDown));
    // Row 41's middle (830 px) at the box's (305 px).
    assert_eq!(offset(&h, 20.0), 525.0);
}

#[test]
fn a_row_that_says_always_is_not_passed_over() {
    let mut h = host();
    place(
        &mut h,
        &list(20.0, 610.0, 100, MANDATORY, START)
            .replace("</style>", "#r10{scroll-snap-stop:always}</style>"),
        "20",
        "40",
    );
    focus(&mut h, "list");
    assert!(key(&mut h, KeyName::PageDown));
    assert_eq!(offset(&h, 20.0), 200.0);
    assert!(key(&mut h, KeyName::PageDown));
    assert_eq!(offset(&h, 20.0), 720.0);
}

#[test]
fn the_root_snaps_from_the_root_elements_properties() {
    // 30 px rows in a surface 80 px tall: the viewport scrolls, and the
    // keys reach it from the page's main, focused.
    let mut h = host();
    let items: String = (0..20).map(|i| format!("<div id=r{i}>{i}</div>")).collect();
    let page = format!(
        "<style>html{{{MANDATORY}}}body{{margin:0}}div{{height:30px;{START}}}</style>\
         <main id=m tabindex=-1>{items}</main>"
    );
    place(&mut h, &page, "20", "4");
    focus(&mut h, "m");
    assert!(key(&mut h, KeyName::Down));
    assert_eq!(offset(&h, 30.0), 30.0);
    assert!(wheel(&mut h, 15.0, 10.0, 50.0));
    assert_eq!(offset(&h, 30.0), 90.0);
    // The root's thumb (hotty-blitz's): 32 px of 80 for 600 px of content,
    // 520 to go in 48. Dragged 3 px from 90, it rests on the nearest row.
    let mut fx = h.pointer("x", PointerKind::Down, 193.0, 20.0, Mods::default());
    fx.extend(h.pointer("x", PointerKind::Move, 193.0, 23.0, Mods::default()));
    let held = offset(&h, 30.0);
    assert!(held > 120.0 && held < 125.0, "dragged to {held}");
    fx.extend(h.pointer("x", PointerKind::Up, 193.0, 23.0, Mods::default()));
    render(&mut h);
    assert_eq!(offset(&h, 30.0), 120.0);
    assert_eq!(differs_from_a_full_paint(&mut h), 0);
    // A body's `scroll-snap-type` does not snap the viewport.
    let mut h = host();
    place(&mut h, &page.replace("html{", "body{"), "20", "4");
    focus(&mut h, "m");
    assert!(key(&mut h, KeyName::Down));
    assert_eq!(offset(&h, 30.0), 40.0);
}

#[test]
fn an_elements_thumb_let_go_rests_on_a_row() {
    // Blitz's thumb on a box 60 px tall that holds 160 px.
    let mut h = host();
    place(&mut h, &list(20.0, 60.0, 8, MANDATORY, START), "20", "4");
    assert!(wheel(&mut h, 15.0, 30.0, 20.0));
    assert_eq!(offset(&h, 20.0), 20.0);
    // The thumb, 32 px long from 20/100 of the 28 px it has to go.
    let mut fx = h.pointer("x", PointerKind::Down, 143.0, 30.0, Mods::default());
    fx.extend(h.pointer("x", PointerKind::Move, 143.0, 40.0, Mods::default()));
    let held = offset(&h, 20.0);
    assert!(
        held > 20.0 && (held / 20.0).fract() > 0.05,
        "dragged between rows: {held}"
    );
    fx.extend(h.pointer("x", PointerKind::Up, 143.0, 40.0, Mods::default()));
    render(&mut h);
    assert_eq!(offset(&h, 20.0), (held / 20.0).round() * 20.0);
    assert_eq!(differs_from_a_full_paint(&mut h), 0);
}

#[test]
fn a_document_without_the_properties_scrolls_as_before() {
    let mut h = host();
    rows_of_20(&mut h, "");
    assert!(key(&mut h, KeyName::Down));
    assert_eq!(offset(&h, 20.0), 40.0);
    assert!(key(&mut h, KeyName::PageDown));
    assert_eq!(offset(&h, 20.0), 573.75);
    assert!(key(&mut h, KeyName::Home));
    assert!(wheel(&mut h, 15.0, 30.0, 45.0));
    assert_eq!(offset(&h, 20.0), 45.0);
    reveal(&mut h, "r0", "start");
    reveal(&mut h, "r30", "");
    assert_eq!(offset(&h, 20.0), 10.0);
    // Snap areas without a snap container, and a snap container without
    // areas, the same.
    let mut h = host();
    place(&mut h, &list(20.0, 610.0, 100, MANDATORY, ""), "20", "40");
    focus(&mut h, "list");
    assert!(key(&mut h, KeyName::PageDown));
    assert_eq!(offset(&h, 20.0), 533.75);
}

/// What Stylo computes and serializes in servo mode, where it parsed none of
/// these before (mods/stylo/scroll-snap.md): `getComputedStyle()`'s values.
#[test]
fn the_snap_properties_parse_and_compute_in_servo_mode() {
    let mut h = host();
    place(
        &mut h,
        "<style>#a{scroll-snap-type:y mandatory;scroll-padding:10px 5%;scroll-snap-align:start}\
         #b{scroll-snap-type:both;scroll-snap-align:center end;scroll-snap-stop:always;\
         scroll-padding-block:4px 6px;scroll-padding-inline-start:3px}\
         #c{scroll-snap-type:inline proximity;scroll-padding-inline:auto 2px}</style>\
         <div id=a></div><div id=b></div><div id=c></div>",
        "20",
        "4",
    );
    let s = |sel: &str, p: &str| h.computed_style("x", sel, p, false);
    assert_eq!(s("#a", "scroll-snap-type"), "y mandatory");
    assert_eq!(s("#a", "scroll-snap-align"), "start");
    assert_eq!(s("#a", "scroll-snap-stop"), "normal");
    assert_eq!(s("#a", "scroll-padding-top"), "10px");
    assert_eq!(s("#a", "scroll-padding-left"), "5%");
    assert_eq!(s("#a", "scroll-padding"), "10px 5%");
    assert_eq!(s("#b", "scroll-snap-type"), "both");
    assert_eq!(s("#b", "scroll-snap-align"), "center end");
    assert_eq!(s("#b", "scroll-snap-stop"), "always");
    assert_eq!(s("#b", "scroll-padding-top"), "4px");
    assert_eq!(s("#b", "scroll-padding-bottom"), "6px");
    assert_eq!(s("#b", "scroll-padding-left"), "3px");
    assert_eq!(s("#b", "scroll-padding-right"), "auto");
    assert_eq!(s("#b", "scroll-padding-block"), "4px 6px");
    assert_eq!(s("#c", "scroll-snap-type"), "inline");
    assert_eq!(s("#c", "scroll-padding-inline"), "auto 2px");
    // The initial values.
    assert_eq!(s("body", "scroll-snap-type"), "none");
    assert_eq!(s("body", "scroll-snap-align"), "none");
    assert_eq!(s("body", "scroll-padding"), "auto");
}

/// The shorthands and longhands as a declaration block serializes them
/// (`cssText`, `getPropertyValue()`), and what does not parse.
#[test]
fn the_snap_properties_serialize_as_declared() {
    use blitz_dom::DocumentConfig;
    use blitz_html::HtmlDocument;
    let doc = HtmlDocument::from_html("<p>", DocumentConfig::default());
    let text = |css: &str| doc.style_attr_serialize(css);
    let value = |css: &str, p: &str| doc.style_attr_get_property(css, p);
    assert_eq!(
        text("scroll-padding: 1px 2px 1px 2px"),
        "scroll-padding: 1px 2px;"
    );
    assert_eq!(
        text("scroll-padding: 1px; scroll-padding-left: auto"),
        "scroll-padding: 1px 1px 1px auto;"
    );
    assert_eq!(
        value("scroll-padding-block: 3px 4%", "scroll-padding-block"),
        "3px 4%"
    );
    assert_eq!(
        value("scroll-padding-inline: 3px", "scroll-padding-inline-end"),
        "3px"
    );
    assert_eq!(
        text("scroll-snap-type: x proximity; scroll-snap-align: end end"),
        "scroll-snap-type: x; scroll-snap-align: end;"
    );
    assert_eq!(
        text("scroll-snap-stop: always"),
        "scroll-snap-stop: always;"
    );
    // Invalid values are dropped, as anywhere.
    assert_eq!(text("scroll-snap-type: mandatory y"), "");
    assert_eq!(text("scroll-padding: -1px"), "");
    assert_eq!(text("scroll-snap-align: left"), "");
}

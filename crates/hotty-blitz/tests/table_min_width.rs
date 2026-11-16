//! A table is never narrower than its content (CSS 2.1 §17.5.2.2, auto table
//! layout: its used width is the greater of `width` and its minimum content
//! width). A `width: 100%` table in a narrow `overflow-x: auto` wrapper
//! overflows it, so the wrapper scrolls sideways, as shadcn's Table does in a
//! browser. A `table-layout: fixed` table keeps the width it is given.

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics, Mods};

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// `html` on a surface of 40 by 5 cells of 10 by 20 px, with wheel scrolling
/// on both axes, rendered.
fn placed(html: &str) -> Host {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x"), ("q", "2"), ("scroll", "3")],
        html,
    ));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "40"),
            ("r", "5"),
            ("q", "2"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    h
}

/// A wrapper 200 px wide holding a table of three cells, each a 50 px box with
/// 20 px of padding either side: 270 px of minimum content. `table` is the
/// table's style, and `outer` the wrapper's.
fn page(outer: &str, table: &str) -> String {
    format!(
        "<style>body{{margin:0;background:#fff}}#w{{width:200px;{outer}}}\
         table{{background:#00f;border-spacing:0;{table}}}td{{padding:0 20px;height:40px;vertical-align:top}}\
         i{{display:block;width:50px;height:20px;background:#f00}}</style>\
         <div id=w><table><tr><td><i></i></td><td><i></i></td><td><i></i></td></tr></table></div>"
    )
}

/// How far right the table's blue reaches below its cells' boxes, in pixels:
/// its width. The wrapper does not clip it: its overflow is visible.
fn table_width(table: &str) -> u32 {
    let h = placed(&page("", table));
    let f = h.frame("x").unwrap();
    (0..f.width)
        .rev()
        .find(|&x| {
            let i = ((30 * f.width + x) * 4) as usize;
            f.rgba[i..i + 3] == [0, 0, 255]
        })
        .map_or(0, |x| x + 1)
}

/// Whether a horizontal wheel over the wrapper scrolls it.
fn wheel_taken(table: &str) -> bool {
    let mut h = placed(&page("overflow-x:auto", table));
    let taken = h.wheel("x", 50.0, 10.0, 60.0, 0.0, Mods::default()).taken;
    h.end_gesture();
    taken
}

#[test]
fn a_table_without_width_is_as_wide_as_its_content() {
    assert_eq!(table_width(""), 270);
    assert!(wheel_taken(""));
}

#[test]
fn a_full_width_table_is_not_narrower_than_its_content() {
    assert_eq!(table_width("width:100%"), 270);
    assert!(wheel_taken("width:100%"));
}

#[test]
fn a_table_wider_than_its_content_keeps_its_width() {
    assert_eq!(table_width("width:350px"), 350);
    assert!(wheel_taken("width:350px"));
    let wide = "<style>body{margin:0}#w{width:400px;overflow-x:auto}table{width:100%}td{white-space:nowrap}</style>\
                <div id=w><table><tr><td>a</td><td>b</td></tr></table></div>";
    let mut h = placed(wide);
    assert!(!h.wheel("x", 50.0, 10.0, 60.0, 0.0, Mods::default()).taken);
    h.end_gesture();
}

#[test]
fn a_fixed_table_has_the_width_it_is_given() {
    assert_eq!(table_width("table-layout:fixed;width:100%"), 200);
    assert!(!wheel_taken("table-layout:fixed;width:100%"));
}

#[test]
fn a_table_in_a_wrapper_that_does_not_scroll_overflows_it_visibly() {
    assert_eq!(table_width("width:100%"), 270);
}

/// A wrapper 200 px wide holding a table of three cells with 10 px of padding
/// either side, laid out as Basecoat's and shadcn's Tasks tables are: a green
/// 50 px box, then a flex row of a 30 px badge and a title truncated at
/// `max-width: 100px` (Tailwind's `truncate`) over 300 px of content, then a
/// red 50 px box. The title adds 100 px to its column, not 300 px, so the
/// table's minimum content is 70 + 160 + 70 = 300 px, as Chromium has it.
/// `table` is the table's style, and `outer` the wrapper's.
fn titles(outer: &str, table: &str) -> String {
    format!(
        "<style>body{{margin:0;background:#fff}}#w{{width:200px;{outer}}}\
         table{{background:#00f;border-spacing:0;{table}}}td{{padding:0 10px;height:40px;vertical-align:top;white-space:nowrap}}\
         i,em{{display:block;width:50px;height:20px;background:#f00}}em{{background:#0f0}}.f{{display:flex;gap:10px}}\
         .f b{{display:block;width:30px;height:20px;background:#0f0}}\
         .t{{max-width:100px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}\
         .t u{{display:inline-block;width:300px;height:20px}}</style>\
         <div id=w><table><tr><td><em></em></td><td><div class=f><b></b><span class=t><u></u></span></div></td>\
         <td><i></i></td></tr></table></div>"
    )
}

/// The runs of `rgb` along row `y` of `h`'s frame, as `[start, end)` pixels.
fn runs(h: &Host, y: u32, rgb: [u8; 3]) -> Vec<(u32, u32)> {
    let f = h.frame("x").unwrap();
    let mut out: Vec<(u32, u32)> = Vec::new();
    for x in 0..f.width {
        let i = ((y * f.width + x) * 4) as usize;
        if f.rgba[i..i + 3] == rgb {
            match out.last_mut() {
                Some(r) if r.1 == x => r.1 = x + 1,
                _ => out.push((x, x + 1)),
            }
        }
    }
    out
}

#[test]
fn a_truncated_title_adds_its_max_width_to_the_table() {
    for table in ["width:100%", ""] {
        let h = placed(&titles("", table));
        assert_eq!(
            runs(&h, 30, [0, 0, 255]),
            [(0, 300)],
            "{table:?}: the table's box"
        );
        // The last column lies inside the table's box, at its right edge.
        assert_eq!(
            runs(&h, 10, [255, 0, 0]),
            [(240, 290)],
            "{table:?}: the last cell"
        );
    }
}

#[test]
fn a_wheel_across_a_table_of_truncated_titles_reaches_its_last_column() {
    let mut h = placed(&titles("overflow-x:auto", "width:100%"));
    assert!(runs(&h, 10, [255, 0, 0]).is_empty(), "past the wrapper");
    // 50 px across, half the overflow: the last cell's left edge comes in.
    // With the title's whole content in its column, it would be 240 px away.
    assert!(h.wheel("x", 50.0, 10.0, 50.0, 0.0, Mods::default()).taken);
    h.end_gesture();
    h.render_dirty(&mut |_, _, _| {});
    assert_eq!(runs(&h, 10, [255, 0, 0]), [(190, 200)]);
    // The rest of the way: the whole last cell is in the wrapper.
    assert!(h.wheel("x", 50.0, 10.0, 1000.0, 0.0, Mods::default()).taken);
    h.end_gesture();
    h.render_dirty(&mut |_, _, _| {});
    assert_eq!(runs(&h, 10, [255, 0, 0]), [(140, 190)]);
}

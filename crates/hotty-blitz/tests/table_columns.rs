//! The columns of an auto-layout table share the width the table has spare in
//! proportion to their sizes, as browsers do (CSS 2.1 section 17.5.2.2 leaves
//! it to the user agent; Chromium and Firefox agree): between the sum of the
//! columns' minimum content widths and the sum of their maximum content
//! widths, in proportion to maximum less minimum; beyond it, in proportion to
//! maximum. A grid shares it equally. The expected widths are Chromium 153's,
//! from the same pages (a column of boxes `w` px wide that may wrap between
//! them has `w`-wide minimum and summed maximum content widths).

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// A cell of boxes, `ws` px wide each.
fn cell(bg: &str, style: &str, ws: &[u32]) -> String {
    let boxes: Vec<String> = ws
        .iter()
        .map(|w| format!("<i style='width:{w}px'></i>"))
        .collect();
    format!(
        "<td style='background:{bg};{style}'>{}</td>",
        boxes.join(" ")
    )
}

/// How wide each colour (red, green, blue: so the cells go in that order) is across the row of the table the page draws, in
/// pixels: its columns' widths. The surface is 100 cells of 10px.
fn columns(table_style: &str, wrapper: u32, cells: &[String]) -> Vec<u32> {
    let html = format!(
        "<style>body{{margin:0;background:#fff}}#w{{width:{wrapper}px}}table{{border-spacing:0;{table_style}}}\
         td{{padding:0;font-size:0;height:20px}}i{{display:inline-block;height:20px}}</style>\
         <div id=w><table><tr>{}</tr></table></div>",
        cells.concat()
    );
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], &html));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "100"),
            ("r", "3"),
            ("q", "2"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    let f = h.frame("x").unwrap();
    [RED, GREEN, BLUE]
        .iter()
        .map(|colour| {
            (0..f.width)
                .filter(|&x| {
                    let i = ((10 * f.width + x) * 4) as usize;
                    f.rgba[i..i + 3] == *colour
                })
                .count() as u32
        })
        .filter(|&w| w > 0)
        .collect()
}

/// `got` is `want` to within a pixel a column (Chromium lays out in 1/64 px).
fn assert_columns(got: Vec<u32>, want: &[u32]) {
    assert_eq!(got.len(), want.len(), "columns: {got:?}, want {want:?}");
    for (g, w) in got.iter().zip(want) {
        assert!(g.abs_diff(*w) <= 1, "columns: {got:?}, want {want:?}");
    }
}

#[test]
fn spare_width_beyond_the_maximum_is_shared_in_proportion_to_it() {
    let cells = [cell("#f00", "", &[100]), cell("#00f", "", &[300])];
    assert_columns(columns("width:100%", 1000, &cells), &[250, 750]);

    let cells = [
        cell("#f00", "", &[50]),
        cell("#0f0", "", &[100]),
        cell("#00f", "", &[350]),
    ];
    assert_columns(columns("width:100%", 1000, &cells), &[100, 200, 700]);
}

#[test]
fn width_between_the_minimum_and_the_maximum_is_shared_in_proportion_to_their_difference() {
    let cells = [cell("#f00", "", &[100, 100]), cell("#00f", "", &[200, 200])];
    assert_columns(columns("width:100%", 450, &cells), &[150, 300]);

    let cells = [
        cell("#f00", "", &[100, 100]),
        cell("#0f0", "", &[50, 50, 50]),
        cell("#00f", "", &[200, 100]),
    ];
    assert_columns(columns("width:100%", 500, &cells), &[150, 100, 250]);
}

#[test]
fn a_table_without_width_shrinks_to_fit_the_same_way() {
    let cells = [cell("#f00", "", &[100, 100]), cell("#00f", "", &[200, 200])];
    assert_columns(columns("", 450, &cells), &[150, 300]);
}

#[test]
fn columns_with_a_width_keep_it_and_the_rest_share_what_is_left() {
    let cells = [
        cell("#f00", "width:200px", &[50]),
        cell("#0f0", "", &[100]),
        cell("#00f", "", &[300]),
    ];
    assert_columns(columns("width:100%", 1000, &cells), &[200, 200, 600]);
}

#[test]
fn a_fixed_table_shares_it_equally() {
    let cells = [cell("#f00", "", &[100]), cell("#00f", "", &[300])];
    assert_columns(
        columns("width:100%;table-layout:fixed", 1000, &cells),
        &[500, 500],
    );
}

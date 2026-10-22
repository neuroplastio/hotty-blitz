//! A non-atomic inline box (a `span` in a line of text) takes the space of
//! its inline-start and inline-end margin, border and padding, as a browser
//! lays it out: `A<i style="margin-left:2ch">x</i>` leaves a gap between
//! the A and the x. Blitz laid the box's text out as if it had none, and a
//! picker row read "New pane(your shell)".

mod common;

use common::delta_cmd;
use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

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

fn render(h: &mut Host) {
    h.render_dirty(&mut |_, _, _| {});
}

/// A surface 500px by 60px showing `body`, white on black but for what `<r>`
/// marks red and `#s`'s blue background, in lines 400px wide.
fn show(body: &str) -> Host {
    let mut h = host();
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x"), ("q", "2")],
        &format!(
            "<style>body{{margin:0;font:20px sans-serif;color:#fff;background:#000}}\
             div{{width:400px}}i{{font-style:normal}}r{{color:#f00}}#s{{background:#246}}</style>{body}"
        ),
    ));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "50"),
            ("r", "3"),
            ("q", "2"),
        ],
        "",
    ));
    render(&mut h);
    h
}

/// The leftmost column the surface draws red in.
fn red_x(h: &Host) -> i64 {
    let f = h.frame("x").unwrap();
    let red = |x: u32, y: u32| {
        let i = ((y * f.width + x) * 4) as usize;
        f.rgba[i] > 128 && f.rgba[i + 1] < 64 && f.rgba[i + 2] < 64
    };
    (0..f.width)
        .find(|&x| (0..f.height).any(|y| red(x, y)))
        .expect("something red") as i64
}

/// How far right the red in `line` sits of the red in `plain`, the same line
/// with no margin, border or padding.
fn shift(line: &str, plain: &str) -> i64 {
    red_x(&show(line)) - red_x(&show(plain))
}

#[test]
fn a_span_s_inline_start_edge_moves_what_follows() {
    let plain = "<div>A<i><r>x</r></i></div>";
    for line in [
        "<div>A<i style='margin-left:40px'><r>x</r></i></div>",
        "<div>A<i style='padding-left:40px'><r>x</r></i></div>",
        "<div>A<i style='margin-left:20px;border-left:5px solid;padding-left:15px'><r>x</r></i></div>",
        // A percentage of the line's width.
        "<div>A<i style='margin-left:10%'><r>x</r></i></div>",
        "<div>A<i style='margin-left:15px'><i style='padding-left:25px'><r>x</r></i></i></div>",
    ] {
        assert_eq!(shift(line, plain), 40, "{line}");
    }
    // An empty span takes its margin too.
    assert_eq!(
        shift(
            "<div>A<i style='margin-left:40px'></i><r>x</r></div>",
            "<div>A<i></i><r>x</r></div>",
        ),
        40
    );
}

#[test]
fn a_span_s_inline_end_edge_moves_what_follows() {
    assert_eq!(
        shift(
            "<div><i style='margin-right:25px;padding-right:15px'>A</i><r>x</r></div>",
            "<div><i>A</i><r>x</r></div>",
        ),
        40
    );
}

#[test]
fn white_space_still_collapses_across_a_span_s_edges() {
    // One space between the A and the x either way: the edge adds its 40px
    // and no space.
    assert_eq!(
        shift(
            "<div>A <i style='margin-left:40px'> <r>x</r></i></div>",
            "<div>A <i> <r>x</r></i></div>",
        ),
        40
    );
    assert_eq!(
        shift(
            "<div>A<i style='margin-right:40px'>B </i> <r>x</r></div>",
            "<div>A<i>B </i> <r>x</r></div>",
        ),
        40
    );
    // Markup indented around the span adds no space either.
    assert_eq!(
        shift(
            "<div>\n  <i style='margin-left:40px'>\n    <r>x</r>\n  </i>\n</div>",
            "<div>\n  <i>\n    <r>x</r>\n  </i>\n</div>",
        ),
        40
    );
}

#[test]
fn a_right_to_left_span_s_edges_are_on_its_right_to_left_sides() {
    // In a right-to-left line, an inline box's margin-left is its end: what
    // follows it moves left.
    assert_eq!(
        shift(
            "<div dir=rtl><i style='margin-left:40px'>א</i><r>ב</r></div>",
            "<div dir=rtl><i>א</i><r>ב</r></div>",
        ),
        -40
    );
}

#[test]
fn an_atomic_inline_keeps_its_margin_once() {
    assert_eq!(
        shift(
            "<div>A<i style='display:inline-block;margin-left:40px'>B</i><r>x</r></div>",
            "<div>A<i style='display:inline-block'>B</i><r>x</r></div>",
        ),
        40
    );
}

/// A span's margin and padding, changed by a restyle alone (a `var` delta,
/// as `:hover` would), lay the line out again as the span gains, resizes and
/// loses its edges, and every partial paint equals a full one.
#[test]
fn a_restyle_of_a_span_s_margin_lays_the_line_out_again() {
    let page = |style: &str| {
        format!(
            "<div>A<i id=s style='{style}'>B</i><r>x</r> and text after it</div>\
             <div>a second line that does not move</div>"
        )
    };
    let mut h = show(&page(
        "margin-left:var(--ml,0px);padding-right:var(--pr,0px)",
    ));
    let start = red_x(&h);
    let (mut ml, mut pr) = ("0px", "0px");
    for (k, v, moved) in [
        ("ml", "40px", 40),
        ("ml", "20px", 20),
        ("pr", "30px", 50),
        ("ml", "0px", 30),
        ("pr", "0px", 0),
    ] {
        if k == "ml" {
            ml = v;
        } else {
            pr = v;
        }
        h.handle(&delta_cmd(
            &[("s", "x"), ("q", "2")],
            "var",
            Some("s"),
            Some(k),
            v,
        ));
        render(&mut h);
        assert_eq!(red_x(&h) - start, moved, "--{k}: {v}");

        let partial = h.frame("x").unwrap().rgba.clone();
        h.repaint();
        render(&mut h);
        let whole = &h.frame("x").unwrap().rgba;
        let differing = partial
            .iter()
            .zip(whole.iter())
            .filter(|(p, q)| p.abs_diff(**q) > 1)
            .count();
        assert_eq!(
            differing, 0,
            "--{k}: {v}: {differing} channels differ from a full paint"
        );

        let fresh = show(&page(&format!("margin-left:{ml};padding-right:{pr}")));
        assert!(
            &fresh.frame("x").unwrap().rgba == whole,
            "--{k}: {v}: the lines differ from a fresh document's"
        );
    }
}

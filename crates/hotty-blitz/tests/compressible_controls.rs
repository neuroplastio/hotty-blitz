//! A form control with a percentage width is a compressible replaced
//! element (CSS Sizing 3 §5.2.2): its min-content contribution resolves
//! that percentage against zero, so as a flex item it shrinks with the
//! row. basecoat's command box puts `width: 100%` on its search field
//! beside a 16px icon; Chromium gives the field the 176px left of a
//! 200px row, and hotty-blitz kept 200, past the row's end.

mod common;

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

fn shown(doc: &str) -> Host {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], doc));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "40"), ("r", "4")],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    h
}

const STYLE: &str = "<style>body{margin:0;font:14px sans-serif}\
    .row{display:flex;align-items:center;gap:8px;width:200px;height:36px}\
    .icon{width:16px;height:16px;flex-shrink:0}\
    #f{width:100%;padding:0;border:0;margin:0;font:inherit}</style>";

/// The control's left edge and width, from its centre and the centre of
/// a 0-wide marker right after it.
fn span(h: &Host) -> (f32, f32) {
    let (centre, _) = h.element_centre("x", "f").unwrap();
    let (end, _) = h.element_centre("x", "end").unwrap();
    let width = 2.0 * (end - centre);
    (end - width, width)
}

#[test]
fn a_percentage_width_control_shrinks_in_a_flex_row() {
    for control in [
        "<input id=f placeholder='Search framework...'>",
        "<input id=f type=search size=60>",
        "<textarea id=f cols=60></textarea>",
    ] {
        let h = shown(&format!(
            "{STYLE}<div class=row><span class=icon></span>{control}\
             <span id=end style='width:0;flex-shrink:0;margin-left:-8px'></span></div>"
        ));
        let (left, width) = span(&h);
        assert_eq!((left, width), (24.0, 176.0), "{control}");
    }
}

/// A fixed width is no percentage: the control keeps its min-content
/// width, the smaller of that width and its intrinsic one, as before.
#[test]
fn a_fixed_width_control_keeps_its_automatic_minimum() {
    let h = shown(&format!(
        "{STYLE}<style>#f{{width:300px}}</style><div class=row><span class=icon></span>\
         <input id=f size=40><span id=end style='width:0;flex-shrink:0;margin-left:-8px'></span></div>"
    ));
    let (left, width) = span(&h);
    assert_eq!(left, 24.0);
    assert!(
        width > 176.0,
        "an input of size 40 at 300px shrank to {width}"
    );
}

/// basecoat's command box clips its search row (`overflow: hidden`), and
/// focus scrolls the field into view, as a browser's does. While the field
/// overflowed the row, focusing it scrolled the box 12px sideways, and the
/// whole popover's content shifted left; the field fits now, and focus
/// moves nothing.
#[test]
fn focusing_the_field_scrolls_no_clipping_box() {
    let mut h = shown(&format!(
        "{STYLE}<div style='width:200px;overflow:hidden'><div class=row>\
         <span class=icon id=icon></span><input id=f>\
         <span id=end style='width:0;flex-shrink:0;margin-left:-8px'></span></div></div>"
    ));
    let before = h.element_centre("x", "icon").unwrap();
    h.handle(&cmd(
        &[("a", "focus"), ("s", "x"), ("t", "f"), ("q", "2")],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    assert_eq!(h.focused_surface(), Some("x"));
    assert_eq!(h.element_centre("x", "icon").unwrap(), before);
    assert_eq!(before.0, 8.0);
}

/// A form control laid out as a block keeps its own width, as Chromium's
/// does (a text field of `size=20` at 14px is far narrower than 400px),
/// where an ordinary block fills the line.
#[test]
fn a_block_control_is_not_stretched() {
    let h = shown(
        "<style>body{margin:0;font:14px sans-serif;width:400px}\
         input,textarea{display:block;margin:0}</style>\
         <input id=i><textarea id=t></textarea><div id=d style='height:10px'></div>",
    );
    let (d, _) = h.element_centre("x", "d").unwrap();
    assert_eq!(d, 200.0);
    for id in ["i", "t"] {
        let (c, _) = h.element_centre("x", id).unwrap();
        assert!(c < 150.0, "{id} centred at {c}: stretched");
    }
}

//! Text fields as a browser draws them: an empty field shows its
//! `placeholder` in its `::placeholder` style, and a number field is as wide
//! as a text field.

mod common;

use common::delta_cmd;
use hotty::Message;
use hotty_blitz::{Config, Host, Key, KeyName, Metrics, Mods};

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

/// `page` on a surface of 20 by 6 cells, rendered.
fn placed(page: &str) -> Host {
    let mut h = host();
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], page));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "20"), ("r", "6")],
        "",
    ));
    render(&mut h);
    h
}

/// The darkest channel between rows `y0..y1` of the surface's last frame.
fn darkest(h: &Host, (y0, y1): (u32, u32)) -> u8 {
    let f = h.frame("x").unwrap();
    (y0..y1)
        .flat_map(|y| (0..f.width).map(move |x| (y, x)))
        .map(|(y, x)| {
            let i = ((y * f.width + x) * 4) as usize;
            *f.rgba[i..i + 3].iter().max().unwrap()
        })
        .min()
        .unwrap()
}

/// Two fields 30px tall, black on white: one whose placeholder is black,
/// and one whose placeholder is black at half opacity.
const PAGE: &str = "<style>body{margin:0;font:20px sans-serif;background:#fff}
    input{display:block;height:30px;border:none;background:transparent;color:#000;font:inherit;padding:0;width:100px}
    ::placeholder{color:#000}.half::placeholder{opacity:.5}</style>
    <input id=a placeholder=HH><input id=b class=half placeholder=HH>";

const A: (u32, u32) = (0, 30);
const B: (u32, u32) = (30, 60);

#[test]
fn an_empty_field_shows_its_placeholder_in_its_style() {
    let h = placed(PAGE);
    assert!(darkest(&h, A) < 64, "the placeholder, in black");
    let half = darkest(&h, B);
    assert!(
        (100..=160).contains(&half),
        "the placeholder at half opacity is grey, not {half}"
    );
    let h = placed(&PAGE.replace("placeholder=HH", "value=''"));
    assert_eq!(darkest(&h, A), 255, "no placeholder, nothing drawn");
}

#[test]
fn a_placeholder_follows_its_attribute() {
    let mut h = placed(&PAGE.replace("placeholder=HH", ""));
    assert_eq!(darkest(&h, A), 255);
    h.handle(&delta_cmd(
        &[("s", "x")],
        "attr",
        Some("a"),
        Some("placeholder"),
        "HH",
    ));
    render(&mut h);
    assert!(darkest(&h, A) < 64, "the placeholder set by a delta");
    h.handle(&delta_cmd(
        &[("s", "x")],
        "unattr",
        Some("a"),
        Some("placeholder"),
        "",
    ));
    render(&mut h);
    assert_eq!(darkest(&h, A), 255, "the placeholder taken away");
}

/// Typing hides the placeholder and deleting what was typed shows it again,
/// each paint drawing what a full paint does.
#[test]
fn typing_hides_the_placeholder() {
    let mut h = placed(&PAGE.replace("placeholder=HH>", "placeholder=HHHHHH>"));
    h.handle(&cmd(&[("a", "focus"), ("s", "x"), ("t", "a")], ""));
    render(&mut h);
    let shown = h.frame("x").unwrap().rgba.clone();
    for (name, step) in [
        (KeyName::Char(".".into()), "typed"),
        (KeyName::Backspace, "deleted"),
    ] {
        assert!(h.key("x", &Key::new(name, Mods::default())).consumed);
        render(&mut h);
        let painted = h.frame("x").unwrap().rgba.clone();
        h.repaint();
        render(&mut h);
        let unpainted = painted
            .iter()
            .zip(&h.frame("x").unwrap().rgba)
            .filter(|(p, q)| p.abs_diff(**q) > 1)
            .count();
        assert_eq!(
            unpainted, 0,
            "{step}: {unpainted} channels differ from a full paint"
        );
        if step == "typed" {
            let f = h.frame("x").unwrap();
            let inked = (0..30)
                .flat_map(|y| (10..100).map(move |x| ((y * f.width + x) * 4) as usize))
                .filter(|&i| f.rgba[i..i + 3] != [255, 255, 255])
                .count();
            assert_eq!(inked, 0, "typed: {inked} pixels of the placeholder left");
        }
    }
    // The caret blinks, so only the placeholder's own pixels are compared.
    let f = h.frame("x").unwrap();
    let differing = (0..30)
        .flat_map(|y| (10..100).map(move |x| ((y * f.width + x) * 4) as usize))
        .filter(|&i| shown[i..i + 3] != f.rgba[i..i + 3])
        .count();
    assert_eq!(differing, 0, "{differing} pixels of the placeholder differ");
}

/// The right edge of the border of the field in rows `y0..y1`.
fn right_edge(h: &Host, (y0, y1): (u32, u32)) -> Option<u32> {
    let f = h.frame("x").unwrap();
    (0..f.width).rev().find(|&x| {
        (y0..y1).any(|y| {
            let i = ((y * f.width + x) * 4) as usize;
            f.rgba[i..i + 3].iter().all(|&c| c < 128)
        })
    })
}

#[test]
fn an_empty_number_field_is_as_wide_as_a_text_field() {
    let h = placed(
        "<style>body{margin:0;font:12px sans-serif;background:#fff}
        input{display:block;border:1px solid #000;margin:0 0 20px}</style>
        <input><input type=number>",
    );
    let f = h.frame("x").unwrap();
    let text = right_edge(&h, (0, 15)).expect("the text field");
    let number = right_edge(&h, (45, f.height)).expect("the number field");
    assert!(text > 100, "a text field of 20 characters, not {text}px");
    assert_eq!(number, text, "the number field ends at {number}px");
}

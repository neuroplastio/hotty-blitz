//! Baselines: a single-line text input lines its text up with the text
//! beside it, in a flex row with `align-items: baseline` and in a line, as
//! a browser does, not its box's bottom edge.

use hotty::Message;
use hotty_blitz::{Config, Damage, Host, Key, KeyName, Metrics, Mods};

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

/// The lowest row with ink (a dark pixel) between rows `y0..y1` and columns
/// `x0..x1` of the surface's last frame: where text of capitals sits.
fn ink_bottom(h: &Host, s: &str, (y0, y1): (u32, u32), (x0, x1): (u32, u32)) -> Option<u32> {
    let f = h.frame(s).unwrap();
    (y0..y1).rev().find(|&y| {
        (x0..x1).any(|x| {
            let i = ((y * f.width + x) * 4) as usize;
            f.rgba[i..i + 3].iter().all(|&c| c < 128)
        })
    })
}

/// Three rows 60px tall: "HH" in the first 50px, then an input holding
/// "HH" from x = 50. Two flex rows with `align-items: baseline`, one with a
/// bare input and one with a tall padded one, whose text is centred in it;
/// then a line with the input inline.
const PAGE: &str = "<style>body{margin:0;font:20px sans-serif;color:#000;background:#fff}
    .row{height:60px;margin:0}.flex{display:flex;align-items:baseline}
    .label{display:inline-block;width:50px}
    input{border:none;background:transparent;color:#000;font:inherit;padding:0;width:60px}
    .tall{height:44px;padding:3px;border:2px solid #fff}</style>
    <div class='row flex'><span class=label>HH</span><input id=a value=HH></div>
    <div class='row flex'><span class=label>HH</span><input id=b class=tall value=HH></div>
    <p class=row><span class=label>HH</span><input id=c value=HH></p>";

const ROWS: [(u32, u32); 3] = [(0, 60), (60, 120), (120, 180)];

/// `page` on a surface of `cols` by `rows` cells, rendered.
fn placed(page: &str, cols: u32, rows: u32) -> Host {
    let mut h = host();
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], page));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", &cols.to_string()),
            ("r", &rows.to_string()),
        ],
        "",
    ));
    render(&mut h);
    h
}

#[test]
fn a_text_input_aligns_its_text_on_the_baseline() {
    let h = placed(PAGE, 20, 9);
    for (i, row) in ROWS.into_iter().enumerate() {
        let label = ink_bottom(&h, "x", row, (0, 45)).expect("the label's text");
        let input = ink_bottom(&h, "x", row, (50, 110)).expect("the input's text");
        assert!(
            label.abs_diff(input) <= 1,
            "row {i}: the label's text ends at y = {label}, the input's at y = {input}"
        );
    }
}

/// Text in a fallback font with a taller line (CJK) moves the input's
/// baseline, and the label aligned with it, as typing it does in a browser:
/// typed in, the page is the one that held the text from the start. Each
/// keystroke paints part of a surface large enough for partial paints, and
/// draws what a full paint does, the moved label too.
#[test]
fn typing_moves_the_baseline_as_the_text_would_from_the_start() {
    const TYPED: &str = "日本";
    let mut h = placed(&PAGE.replace("value=HH", "value=''"), 120, 40);
    for id in ["a", "b", "c"] {
        h.handle(&cmd(&[("a", "focus"), ("s", "x"), ("t", id)], ""));
        render(&mut h);
        for c in TYPED.chars() {
            let key = Key::new(KeyName::Char(c.to_string()), Mods::default());
            assert!(h.key("x", &key).consumed);
            let mut partial = false;
            h.render_dirty(&mut |_, _, d| partial = matches!(d, Damage::Rects(_)));
            assert!(partial, "{id} {c:?}: a partial paint");
            let painted = h.frame("x").unwrap().rgba.clone();
            h.repaint();
            render(&mut h);
            // A partial paint draws the scene from another origin, where
            // an anti-aliased edge can differ by one level.
            let unpainted = painted
                .iter()
                .zip(&h.frame("x").unwrap().rgba)
                .filter(|(p, q)| p.abs_diff(**q) > 1)
                .count();
            assert_eq!(
                unpainted, 0,
                "{id} {c:?}: {unpainted} channels differ from a full paint"
            );
        }
    }
    h.blur("x");
    render(&mut h);
    let fresh = placed(
        &PAGE.replace("value=HH", &format!("value={TYPED}")),
        120,
        40,
    );
    let (a, b) = (h.frame("x").unwrap(), fresh.frame("x").unwrap());
    let differing = a
        .rgba
        .chunks(4)
        .zip(b.rgba.chunks(4))
        .filter(|(p, q)| p != q)
        .count();
    assert_eq!(
        differing, 0,
        "{differing} pixels differ from a page with the text from the start"
    );
}

/// A flex item holding an inline-flex button on its line is as tall as that
/// line is in a block: the button sits on its text's baseline while the
/// column measures the item too, not on its bottom edge.
#[test]
fn a_flex_item_measures_an_inline_flex_box_on_its_baseline() {
    let page = |display: &str| {
        format!(
            "<style>body{{margin:0;background:#000;font:16px/24px sans-serif}}
            .col{{display:{display};flex-direction:column}}.item{{background:#c00}}
            button{{display:inline-flex;align-items:center;height:36px;padding:0 12px;border:0;
                margin:0;font:14px/20px sans-serif;background:#06c;color:#fff}}
            p{{margin:0;background:#080;height:20px}}</style>
            <div class=col><div class=item><button>button</button></div><p></p></div>"
        )
    };
    let (flex, block) = (placed(&page("flex"), 30, 4), placed(&page("block"), 30, 4));
    let (a, b) = (flex.frame("x").unwrap(), block.frame("x").unwrap());
    let differing = a
        .rgba
        .chunks(4)
        .zip(b.rgba.chunks(4))
        .filter(|(p, q)| p != q)
        .count();
    assert_eq!(differing, 0, "{differing} pixels differ from the block");
}

/// A fieldset's rendered legend is laid out outside the box its content is
/// laid out in: a flex or grid fieldset's `gap` is between its content's
/// items, not after the legend, which is as wide as its text, and the
/// fieldset's padding is around its content, below the legend. It draws as
/// the same boxes written out do.
#[test]
fn a_fieldset_lays_its_legend_out_outside_its_content() {
    const STYLE: &str = "<style>body{margin:0;background:#000;font:16px/20px sans-serif}
        fieldset,.fs{border:0;margin:0;padding:6px 10px;background:#333;min-inline-size:auto}
        legend,.lg{padding:0;margin:0;background:#c00;color:#fff}
        .lg{display:block;width:fit-content}
        p{margin:0;background:#06c;height:20px}
        .col{display:flex;flex-direction:column;gap:40px}
        .grid{display:grid;gap:12px;grid-template-columns:1fr 1fr}
        fieldset.tight{gap:8px;padding:2px}</style>";
    for (fieldset, written_out) in [
        (
            "<fieldset class=col><legend>legend</legend><p></p><p></p></fieldset>",
            "<div class=fs style=padding:0><span class=lg>legend</span>
                <div class=col style='padding:6px 10px'><p></p><p></p></div></div>",
        ),
        (
            "<fieldset class=grid><p></p><legend>legend</legend><p></p></fieldset>",
            "<div class=fs style=padding:0><span class=lg>legend</span>
                <div class=grid style='padding:6px 10px'><p></p><p></p></div></div>",
        ),
        (
            "<fieldset><legend>legend</legend><p></p></fieldset>",
            "<div class=fs style=padding:0><span class=lg>legend</span>
                <div style='padding:6px 10px;display:flow-root'><p></p></div></div>",
        ),
    ] {
        let a = placed(&format!("{STYLE}{fieldset}"), 30, 8);
        let b = placed(&format!("{STYLE}{written_out}"), 30, 8);
        let (fa, fb) = (a.frame("x").unwrap(), b.frame("x").unwrap());
        let differing = fa
            .rgba
            .chunks(4)
            .zip(fb.rgba.chunks(4))
            .filter(|(p, q)| p != q)
            .count();
        assert_eq!(differing, 0, "{fieldset}: {differing} pixels differ");
    }
    // A class that changes the fieldset's gap and padding restyles its content
    // box too, and it repaints as one drawn so from the start.
    let page = |class: &str| {
        format!(
            "{STYLE}<fieldset id=f class='{class}'><legend>legend</legend><p></p><p></p></fieldset>"
        )
    };
    let mut h = placed(&page("col"), 30, 8);
    let body = rmp_serde::to_vec_named(&serde_json::json!({
        "ops": [{"op": "attr", "t": "f", "k": "class", "v": "col tight"}]
    }))
    .unwrap();
    h.handle(&Message::new(
        [("a", "delta"), ("s", "x")].into_iter().collect(),
        body,
    ));
    render(&mut h);
    let fresh = placed(&page("col tight"), 30, 8);
    let (fa, fb) = (h.frame("x").unwrap(), fresh.frame("x").unwrap());
    let differing = fa
        .rgba
        .chunks(4)
        .zip(fb.rgba.chunks(4))
        .filter(|(p, q)| p != q)
        .count();
    assert_eq!(differing, 0, "restyled: {differing} pixels differ");
}

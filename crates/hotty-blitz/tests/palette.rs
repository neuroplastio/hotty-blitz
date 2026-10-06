//! The host stylesheet's palette (SPEC §8): controls, focus, links and
//! selected text in the terminal's colours, on a dark scheme and a light
//! one, and the document's own rules over them. The shared vectors check
//! the wire, not computed styles, so the host checks them here.
use hotty_blitz::{Config, Host, Metrics, Rgb, Theme};
use hotty_wire::Command;

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Command {
    Command::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// A palette whose ansi-4 and ansi-12 differ, so the accent shows which.
fn theme(dark: bool) -> Theme {
    let (fg, bg) = if dark {
        ("#abb2bf", "#282c34")
    } else {
        ("#383a42", "#fafafa")
    };
    let mut palette = Theme::default().palette;
    palette[4] = Rgb::parse("#0000aa").unwrap();
    palette[8] = Rgb::parse("#808080").unwrap();
    palette[12] = Rgb::parse("#5555ff").unwrap();
    Theme {
        fg: Rgb::parse(fg).unwrap(),
        bg: Rgb::parse(bg).unwrap(),
        palette,
        dark,
    }
}

fn rgb(c: Rgb) -> String {
    format!("rgb({}, {}, {})", c.0, c.1, c.2)
}

const FORM: &str = "<style>#own { background: rgb(1, 2, 3); } #own:focus-visible { outline: none; }</style>\
    <input id=t> <input id=d disabled value=x> <textarea id=ta></textarea> <select id=s><option>a</select>\
    <button id=b>Go</button> <button id=bd disabled>Off</button> <input id=sub type=submit value=S>\
    <input id=cb type=checkbox> <a id=a href='#x'>link</a> <p id=p>text</p> <button id=own>Mine</button>";

/// A document placed on a host with `theme`, drawn.
fn host(theme: Theme, scroll: Option<&str>) -> Host {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        theme,
        ..Config::default()
    });
    let mut doc = vec![("a", "doc"), ("s", "x"), ("q", "2")];
    if let Some(v) = scroll {
        doc.push(("scroll", v));
    }
    h.handle(&cmd(&doc, FORM));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "60"),
            ("r", "8"),
            ("q", "2"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    h
}

fn focus(h: &mut Host, id: &str) {
    h.handle(&cmd(
        &[("a", "focus"), ("s", "x"), ("t", id), ("q", "2")],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
}

fn style(h: &Host, selector: &str, property: &str) -> String {
    h.computed_style("x", selector, property, false)
}

fn controls_take_the_palette(dark: bool) {
    let t = theme(dark);
    let (fg, bg, dim) = (rgb(t.fg), rgb(t.bg), rgb(t.palette[8]));
    let accent = rgb(t.palette[if dark { 12 } else { 4 }]);
    let mut h = host(t, None);
    let s = |h: &Host, sel: &str, p: &str| style(h, sel, p);

    for field in ["#t", "#ta", "#s"] {
        assert_eq!(s(&h, field, "color"), fg, "{field}");
        assert_eq!(s(&h, field, "background-color"), bg, "{field}");
        assert_eq!(s(&h, field, "border-top-color"), dim, "{field}");
        assert_eq!(s(&h, field, "border-top-style"), "solid", "{field}");
        assert_eq!(s(&h, field, "border-top-width"), "1px", "{field}");
    }
    assert_eq!(s(&h, "#d", "color"), dim, "disabled text is dim");
    assert_eq!(s(&h, "#d", "background-color"), bg);
    assert_eq!(s(&h, "#b", "background-color"), dim, "a button is ansi-8");
    assert_eq!(s(&h, "#b", "color"), fg);
    assert_eq!(
        s(&h, "#sub", "background-color"),
        dim,
        "so is a submit input"
    );
    assert_eq!(
        s(&h, "#bd", "background-color"),
        bg,
        "a disabled one keeps the background"
    );
    assert_eq!(s(&h, "#bd", "color"), dim);
    assert_ne!(
        s(&h, "#cb", "border-top-color"),
        dim,
        "checks are not fields"
    );
    assert_eq!(s(&h, "#a", "color"), accent, "links take the accent");
    assert_eq!(
        h.computed_style("x", "#p", "color", true),
        bg,
        "selected text"
    );
    assert_eq!(
        h.computed_style("x", "#p", "background-color", true),
        accent
    );

    // Focus from the program (or the keyboard) shows the ring.
    for id in ["t", "b"] {
        focus(&mut h, id);
        assert_eq!(s(&h, &format!("#{id}"), "outline-style"), "solid", "#{id}");
        assert_eq!(s(&h, &format!("#{id}"), "outline-width"), "1px");
        assert_eq!(s(&h, &format!("#{id}"), "outline-color"), accent);
    }
    assert_eq!(s(&h, "#t", "outline-style"), "none", "only what has focus");

    // The document's own rules win, however specific the host's.
    assert_eq!(s(&h, "#own", "background-color"), "rgb(1, 2, 3)");
    focus(&mut h, "own");
    assert_eq!(s(&h, "#own", "outline-style"), "none");
}

#[test]
fn controls_take_the_palette_on_a_dark_scheme() {
    controls_take_the_palette(true);
}

#[test]
fn controls_take_the_palette_on_a_light_scheme() {
    controls_take_the_palette(false);
}

#[test]
fn the_root_clips_along_the_axes_a_document_did_not_ask_for() {
    let overflow = |scroll: Option<&str>| {
        let h = host(theme(true), scroll);
        (
            style(&h, ":root", "overflow-x"),
            style(&h, ":root", "overflow-y"),
        )
    };
    let s = |a: &str, b: &str| (a.to_string(), b.to_string());
    assert_eq!(overflow(None), s("hidden", "hidden"));
    assert_eq!(overflow(Some("1")), s("hidden", "auto"));
    assert_eq!(overflow(Some("2")), s("auto", "hidden"));
    assert_eq!(overflow(Some("3")), s("visible", "visible"));
}

#[test]
fn selected_text_is_painted_in_the_accent() {
    let t = theme(true);
    let (accent, bg) = (t.palette[12], t.bg);
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        theme: t,
        ..Config::default()
    });
    h.handle(&cmd(
        &[("a", "doc"), ("s", "x"), ("q", "2")],
        "<p style='margin:0'>select these words here</p>",
    ));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "30"),
            ("r", "2"),
            ("q", "2"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    let mods = hotty_blitz::Mods::default();
    h.pointer("x", hotty_blitz::PointerKind::Down, 2.0, 10.0, mods);
    h.pointer("x", hotty_blitz::PointerKind::Move, 150.0, 10.0, mods);
    h.render_dirty(&mut |_, _, _| {});
    let f = h.frame("x").unwrap();
    let px = |x: u32, y: u32| {
        let i = ((y * f.width + x) * 4) as usize;
        Rgb(f.rgba[i], f.rgba[i + 1], f.rgba[i + 2])
    };
    let near = |a: Rgb, b: Rgb| {
        (a.0 as i32 - b.0 as i32).abs() <= 8
            && (a.1 as i32 - b.1 as i32).abs() <= 8
            && (a.2 as i32 - b.2 as i32).abs() <= 8
    };
    // The highlight: the accent, from the press to the pointer.
    let lit: Vec<u32> = (0..f.width).filter(|&x| px(x, 2) == accent).collect();
    assert!(
        lit.len() > 100,
        "the highlight is the accent ({} px)",
        lit.len()
    );
    let (x0, x1) = (lit[0], *lit.last().unwrap());
    // The selected glyphs on it: the background colour, not the text's.
    let glyphs = (x0..=x1)
        .flat_map(|x| (0..20).map(move |y| (x, y)))
        .filter(|&(x, y)| near(px(x, y), bg))
        .count();
    assert!(
        glyphs > 50,
        "selected text is drawn in the background colour ({glyphs} px)"
    );
}

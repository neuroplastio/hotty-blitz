//! A `cid:` stylesheet is parsed once for every document of the session
//! that links it (SPEC §7.1), and what documents draw with it is what they
//! would draw had each parsed it: sent again, deleted, imported, or under a
//! `media` query, and in another session under the same id.

mod common;

use hotty::Message;
use hotty::host::{Output, Splitter};
use hotty_blitz::{Config, Effect, Host, Metrics};

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

fn cmd(h: &mut Host, pairs: &[(&str, &str)], payload: &str) -> Vec<Message> {
    let effects = h.handle(&Message::new(
        pairs.iter().copied().collect(),
        payload.as_bytes().to_vec(),
    ));
    let mut out = Vec::new();
    let mut s = Splitter::with_max(hotty_blitz::MAX_COMMAND_BYTES);
    for e in &effects {
        if let Effect::Reply(b) = e {
            s.feed(b, &mut |ev| {
                if let Output::Command(c) = ev {
                    out.push(c);
                }
            });
        }
    }
    out
}

fn res(h: &mut Host, id: &str, css: &str) {
    let r = cmd(
        h,
        &[("a", "res"), ("id", id), ("type", "text/css"), ("n", "1")],
        css,
    );
    assert_eq!(r[0].get("a"), Some("ok"));
}

/// Replaces surface `s` with `html` (the warm `a=doc` a tab switch is),
/// placed 10×2 cells the first time, and draws it.
fn doc(h: &mut Host, s: &str, html: &str) {
    let placed = h.frame(s).is_some();
    cmd(h, &[("a", "doc"), ("s", s)], html);
    if !placed {
        cmd(h, &[("a", "place"), ("s", s), ("c", "10"), ("r", "2")], "");
    }
    draw(h);
}

fn draw(h: &mut Host) {
    h.render_dirty(&mut |_, _, _| {});
}

fn pixel(h: &Host, s: &str, x: u32, y: u32) -> [u8; 3] {
    let f = h.frame(s).unwrap();
    let i = ((y * f.width + x) * 4) as usize;
    [f.rgba[i], f.rgba[i + 1], f.rgba[i + 2]]
}

const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 128, 0];
const BLUE: [u8; 3] = [0, 0, 255];

/// A body that draws the sheet's `html` background, with the link after
/// its content (in `<head>`, a sheet not sent yet holds back painting).
fn linking(href: &str, extra: &str) -> String {
    format!(
        "<body style='margin:0'><p id=t class=t>text</p>\
         <link id=l rel=stylesheet href='{href}'{extra}></body>"
    )
}

const SHEET: &str = "html{background:#f00} p.t{color:#00f;font-weight:bold;\
                     border:1px solid lime;margin:2px} @media (min-width:1px){p.t{padding:1px}}";

#[test]
fn a_shared_sheet_draws_what_the_document_would_draw_parsing_it() {
    let mut h = host();
    res(&mut h, "theme", SHEET);
    doc(&mut h, "x", &linking("cid:theme", ""));
    doc(&mut h, "x", &linking("cid:theme", " data-n=2"));
    doc(&mut h, "y", &linking("cid:theme", ""));
    assert_eq!(h.sheets_parsed(), 1, "parsed once for three documents");
    let shared = h.frame("y").unwrap().rgba.clone();
    // The same rules from a `<style>`, which each document parses.
    doc(
        &mut h,
        "z",
        &format!("<body style='margin:0'><p id=t class=t>text</p><style>{SHEET}</style></body>"),
    );
    assert_eq!(h.sheets_parsed(), 1);
    assert_eq!(pixel(&h, "y", 99, 39), RED);
    assert!(h.frame("z").unwrap().rgba == shared, "same pixels");
}

#[test]
fn a_sheet_sent_again_or_deleted_is_parsed_again() {
    let mut h = host();
    res(&mut h, "theme", "html{background:#f00}");
    doc(&mut h, "x", &linking("cid:theme", ""));
    doc(&mut h, "y", &linking("cid:theme", ""));
    assert_eq!(pixel(&h, "x", 99, 39), RED);
    assert_eq!(h.sheets_parsed(), 1);
    // Sent again: every surface that links it draws the new one (§7.1).
    res(&mut h, "theme", "html{background:#00f}");
    draw(&mut h);
    assert_eq!(pixel(&h, "x", 99, 39), BLUE);
    assert_eq!(pixel(&h, "y", 99, 39), BLUE);
    assert_eq!(h.sheets_parsed(), 2, "parsed again once, for both");
    doc(&mut h, "x", &linking("cid:theme", ""));
    assert_eq!(pixel(&h, "x", 99, 39), BLUE);
    assert_eq!(h.sheets_parsed(), 2);
    // Deleted: a new document finds nothing, not the parsed sheet.
    cmd(&mut h, &[("a", "del"), ("id", "theme")], "");
    doc(&mut h, "x", &linking("cid:theme", ""));
    assert_ne!(pixel(&h, "x", 99, 39), BLUE);
    // And the same bytes sent after that are parsed again.
    res(&mut h, "theme", "html{background:#00f}");
    draw(&mut h);
    assert_eq!(pixel(&h, "x", 99, 39), BLUE);
    assert_eq!(h.sheets_parsed(), 3);
}

#[test]
fn sessions_with_the_same_id_do_not_share_a_sheet() {
    let mut a = host();
    let mut b = host();
    res(&mut a, "theme", "html{background:#f00}");
    res(&mut b, "theme", "html{background:#00f}");
    doc(&mut a, "x", &linking("cid:theme", ""));
    doc(&mut b, "x", &linking("cid:theme", ""));
    assert_eq!(pixel(&a, "x", 99, 39), RED);
    assert_eq!(pixel(&b, "x", 99, 39), BLUE);
    assert_eq!((a.sheets_parsed(), b.sheets_parsed()), (1, 1));
    // A reset is a new session: the id is free again, its sheet gone.
    a.reset();
    doc(&mut a, "x", &linking("cid:theme", ""));
    assert_ne!(pixel(&a, "x", 99, 39), RED);
    res(&mut a, "theme", "html{background:#008000}");
    draw(&mut a);
    assert_eq!(pixel(&a, "x", 99, 39), GREEN);
    assert_eq!(a.sheets_parsed(), 1);
    assert_eq!(pixel(&b, "x", 99, 39), BLUE);
}

#[test]
fn a_sheet_is_shared_with_its_imports_and_parsed_again_when_one_is_sent_again() {
    let mut h = host();
    res(&mut h, "part", "html{background:#f00}");
    res(&mut h, "theme", "@import url(cid:part); p.t{color:#00f}");
    doc(&mut h, "x", &linking("cid:theme", ""));
    doc(&mut h, "y", &linking("cid:theme", ""));
    assert_eq!(pixel(&h, "x", 99, 39), RED, "the import applies");
    assert_eq!(pixel(&h, "y", 99, 39), RED);
    assert_eq!(h.sheets_parsed(), 1);
    // The import sent again reaches every surface whose sheet imports it.
    res(&mut h, "part", "html{background:#00f}");
    draw(&mut h);
    assert_eq!(pixel(&h, "x", 99, 39), BLUE);
    assert_eq!(pixel(&h, "y", 99, 39), BLUE);
    assert_eq!(h.sheets_parsed(), 2);
}

#[test]
fn a_sheet_whose_import_has_not_arrived_is_parsed_by_each_document() {
    let mut h = host();
    res(&mut h, "theme", "@import url(cid:later); p.t{color:#00f}");
    doc(&mut h, "x", &linking("cid:theme", ""));
    assert_ne!(pixel(&h, "x", 99, 39), RED);
    assert_eq!(h.sheets_parsed(), 0, "not shared");
    // The import arrives: the document that waits for it draws it.
    res(&mut h, "later", "html{background:#f00}");
    draw(&mut h);
    assert_eq!(pixel(&h, "x", 99, 39), RED);
    // From now on the sheet is complete, and shared.
    doc(&mut h, "y", &linking("cid:theme", ""));
    doc(&mut h, "y", &linking("cid:theme", " data-n=2"));
    assert_eq!(pixel(&h, "y", 99, 39), RED);
    assert_eq!(h.sheets_parsed(), 1);
}

#[test]
fn each_link_keeps_its_own_media_on_a_shared_sheet() {
    let mut h = host();
    res(&mut h, "theme", "html{background:#f00}");
    doc(
        &mut h,
        "x",
        &linking("cid:theme", " media='(max-width: 10px)'"),
    );
    doc(
        &mut h,
        "y",
        &linking("cid:theme", " media='(min-width: 10px)'"),
    );
    doc(&mut h, "z", &linking("cid:theme", ""));
    assert_eq!(h.sheets_parsed(), 1);
    assert_ne!(pixel(&h, "x", 99, 39), RED, "its media does not match");
    assert_eq!(pixel(&h, "y", 99, 39), RED);
    assert_eq!(pixel(&h, "z", 99, 39), RED);
    // A link's media changed by a delta applies to that document alone.
    h.handle(&common::delta_cmd(
        &[("s", "x")],
        "attr",
        Some("l"),
        Some("media"),
        "(min-width: 10px)",
    ));
    h.handle(&common::delta_cmd(
        &[("s", "z")],
        "attr",
        Some("l"),
        Some("media"),
        "(max-width: 10px)",
    ));
    draw(&mut h);
    assert_eq!(pixel(&h, "x", 99, 39), RED);
    assert_eq!(pixel(&h, "y", 99, 39), RED);
    assert_ne!(pixel(&h, "z", 99, 39), RED);
    assert_eq!(h.sheets_parsed(), 1);
}

/// Places surface `s` `cols` cells wide, and draws.
fn place(h: &mut Host, s: &str, cols: &str) {
    cmd(h, &[("a", "place"), ("s", s), ("c", cols), ("r", "2")], "");
    draw(h);
}

#[test]
fn documents_with_the_same_sheets_keep_their_own_media_query_results() {
    // Their author cascade data is shared (mods/stylo), but only where the
    // sheet's media queries come out the same.
    let mut h = host();
    res(
        &mut h,
        "theme",
        "html{background:#f00} @media (min-width: 200px){html{background:#00f}}",
    );
    doc(&mut h, "x", &linking("cid:theme", ""));
    cmd(
        &mut h,
        &[("a", "doc"), ("s", "y")],
        &linking("cid:theme", ""),
    );
    place(&mut h, "y", "30");
    assert_eq!(pixel(&h, "x", 99, 39), RED, "100 px wide");
    assert_eq!(pixel(&h, "y", 299, 39), BLUE, "300 px wide");
    // Resized, each takes the other's.
    place(&mut h, "x", "30");
    place(&mut h, "y", "10");
    assert_eq!(pixel(&h, "x", 299, 39), BLUE);
    assert_eq!(pixel(&h, "y", 99, 39), RED);
    // A document with a sheet of its own as well has its rules.
    doc(
        &mut h,
        "z",
        &format!(
            "{}<style>html{{background:#008000}}</style>",
            linking("cid:theme", "")
        ),
    );
    assert_eq!(pixel(&h, "z", 99, 39), GREEN);
    doc(&mut h, "z", &linking("cid:theme", ""));
    assert_eq!(pixel(&h, "z", 99, 39), RED);
    assert_eq!(pixel(&h, "x", 299, 39), BLUE);
}

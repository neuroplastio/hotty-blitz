//! A select's UA arrow follows appearance; an author pseudo-element stays.
mod common;
use common::delta_cmd;
use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

const STYLE: &str = "<style>body{margin:0;background:#222}select{width:180px;height:40px;padding:4px;background:#fff;color:#000;border:0;font:16px sans-serif}.plain{appearance:none}</style>";
fn host(extra: &str, class: &str) -> Host {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    let html = format!(
        "{STYLE}<style>{extra}</style><select id=s class='{class}'><option>Alpha</option><option>Beta</option></select>"
    );
    h.handle(&Message::new(
        [("a", "doc"), ("s", "x")].into_iter().collect(),
        html.into_bytes(),
    ));
    h.handle(&Message::new(
        [("a", "place"), ("s", "x"), ("c", "30"), ("r", "4")]
            .into_iter()
            .collect(),
        Vec::new(),
    ));
    h.render_dirty(&mut |_, _, _| {});
    h
}
fn pixels(h: &Host) -> Vec<u8> {
    h.frame("x").unwrap().rgba.clone()
}
#[test]
fn appearance_none_omits_the_ua_arrow() {
    let plain = pixels(&host("", "plain"));
    let removed = pixels(&host("select::after{content:none}", "plain"));
    let native = pixels(&host("", ""));
    assert!(plain == removed, "appearance:none must remove the UA arrow");
    assert!(native != plain, "the native select still draws its arrow");
    assert!(
        pixels(&host("select::after{color:red}", "plain")) == plain,
        "an author styling the pseudo-element without content does not restore the UA arrow"
    );
}
#[test]
fn author_after_is_preserved_and_appearance_can_change() {
    for css in [
        "select::after{content:'X';grid-area:1 / 2;color:#f00}",
        "select::after{content:'X' !important;grid-area:1 / 2;color:#f00}",
    ] {
        assert!(
            pixels(&host(css, "plain")) == pixels(&host(css, "")),
            "author content must not disappear"
        );
        assert!(
            pixels(&host(css, "plain")) != pixels(&host("", "plain")),
            "author content is visibly drawn"
        );
    }
    let mut h = host("", "");
    let native = pixels(&h);
    h.handle(&delta_cmd(
        &[("s", "x")],
        "attr",
        Some("s"),
        Some("class"),
        "plain",
    ));
    h.render_dirty(&mut |_, _, _| {});
    assert!(
        pixels(&h) == pixels(&host("", "plain")),
        "appearance:none after restyle"
    );
    h.handle(&delta_cmd(
        &[("s", "x")],
        "attr",
        Some("s"),
        Some("class"),
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    assert!(pixels(&h) == native, "native arrow returns after restyle");
}

#[test]
fn author_content_can_be_added_and_removed_later() {
    let css = "select.custom::after{content:'X';grid-area:1 / 2;color:#f00}";
    let mut h = host(css, "plain");
    let plain = pixels(&h);
    h.handle(&delta_cmd(
        &[("s", "x")],
        "attr",
        Some("s"),
        Some("class"),
        "plain custom",
    ));
    h.render_dirty(&mut |_, _, _| {});
    assert!(
        pixels(&h) == pixels(&host(css, "plain custom")),
        "author content appears after restyle"
    );
    h.handle(&delta_cmd(
        &[("s", "x")],
        "attr",
        Some("s"),
        Some("class"),
        "plain",
    ));
    h.render_dirty(&mut |_, _, _| {});
    assert!(
        pixels(&h) == plain,
        "UA arrow stays absent after author content is removed"
    );
}

#[test]
fn reverted_content_uses_the_effective_cascade() {
    let plain = pixels(&host("", "plain"));
    assert!(
        pixels(&host("select::after{content:revert}", "plain")) == plain,
        "reverting to the UA arrow must not bring it back"
    );
    let base = "@layer base {select::after{content:'X';color:red}}";
    let reverted = "@layer base {select::after{content:'X';color:red}} @layer later {select::after{content:revert-layer}}";
    assert!(
        pixels(&host(base, "plain")) == pixels(&host(reverted, "plain")),
        "reverting a layer must retain an earlier authored pseudo-element"
    );
}

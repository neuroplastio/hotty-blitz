//! Tabs advance to CSS tab stops instead of drawing a glyph.

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn render(text: &str, tab_size: &str) -> Vec<u8> {
    let doc = format!(
        "<style>html,body{{margin:0;background:#000;color:#fff}}pre{{margin:0;font:20px monospace;tab-size:{tab_size}}}</style><pre>{text}</pre>"
    );
    let mut host = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    let cmd = |pairs: &[(&str, &str)], text: &str| {
        Message::new(pairs.iter().copied().collect(), text.as_bytes().to_vec())
    };
    host.handle(&cmd(&[("a", "doc"), ("s", "x")], &doc));
    host.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "30"), ("r", "4")],
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    host.frame("x").unwrap().rgba.clone()
}

#[test]
fn tabs_follow_the_configured_number_of_space_stops() {
    assert!(
        render("A\tB", "4") == render("A   B", "4"),
        "one character and a tab should reach the fourth space stop"
    );
    assert!(
        render("AB\tC", "8") == render("AB      C", "8"),
        "two characters and a tab should reach the eighth space stop"
    );
}

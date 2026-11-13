use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn render(markup: &str) -> Vec<u8> {
    let doc = format!(
        "<style>html,body{{margin:0;background:black;color:white;font:20px monospace}}</style>{markup}"
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
fn hidden_inline_text_keeps_its_space_and_visible_children_paint() {
    assert!(
        render("A<span style='visibility:hidden'>B</span>C")
            == render("A<span style='color:transparent'>B</span>C")
    );
    assert!(
        render(
            "A<span style='visibility:hidden'>B<span style='visibility:visible'>C</span>D</span>E"
        ) == render(
            "A<span style='color:transparent'>B<span style='color:white'>C</span>D</span>E"
        )
    );
}

//! Pseudo-elements on nested inline boxes participate in the inline text.

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn render(html: &str) -> Vec<u8> {
    let cmd = |pairs: &[(&str, &str)], text: &str| {
        Message::new(pairs.iter().copied().collect(), text.as_bytes().to_vec())
    };
    let mut host = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    host.handle(&cmd(&[("a", "doc"), ("s", "x")], html));
    host.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "20"), ("r", "3")],
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    host.frame("x").unwrap().rgba.clone()
}

#[test]
fn before_and_after_on_nested_inlines_render_in_order() {
    let style = "<style>html,body{margin:0;background:#000;color:#fff}i{font-style:normal}i::before{content:'A'}i::after{content:'B'}</style>";
    let actual = render(&format!("{style}<p><span><i>X</i></span></p>"));
    let expected = render(
        "<style>html,body{margin:0;background:#000;color:#fff}i{font-style:normal}</style><p><span><i>AXB</i></span></p>",
    );
    assert!(
        actual == expected,
        "nested inline pseudo-elements should match literal text"
    );
}

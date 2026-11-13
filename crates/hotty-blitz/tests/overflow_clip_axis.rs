//! `overflow: clip` clips only its own axis when the other is visible.

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

#[test]
fn horizontal_clip_preserves_vertical_overflow() {
    let mut host = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    let doc = "<style>html,body{margin:0;background:#000}.clip{position:relative;width:20px;height:20px;overflow-x:clip}.child{position:absolute;left:10px;top:10px;width:20px;height:30px;background:#f00}</style><div class=clip><div class=child></div></div>";
    let cmd = |pairs: &[(&str, &str)], text: &str| {
        Message::new(pairs.iter().copied().collect(), text.as_bytes().to_vec())
    };
    host.handle(&cmd(&[("a", "doc"), ("s", "x")], doc));
    host.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "10"), ("r", "4")],
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    let frame = host.frame("x").unwrap();
    let pixel = |x: u32, y: u32| frame.rgba[((y * frame.width + x) * 4) as usize];
    assert_eq!(
        pixel(15, 35),
        255,
        "vertical overflow should remain visible"
    );
    assert_eq!(pixel(25, 15), 0, "horizontal overflow should be clipped");
}

#[test]
fn clip_axis_stays_non_scrollable_beside_hidden_axis() {
    let mut doc = blitz_html::HtmlDocument::from_html(
        "<div id='x' style='overflow-x:hidden;overflow-y:clip'>content</div>",
        blitz_dom::DocumentConfig::default(),
    );
    doc.resolve(0.0);
    let node = doc.get_node(doc.get_element_by_id("x").unwrap()).unwrap();
    let styles = node.primary_styles().unwrap();
    assert_eq!(format!("{:?}", styles.get_box().overflow_y), "Clip");
}

#[test]
fn vertical_clip_preserves_horizontal_overflow() {
    let mut host = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    let doc = "<style>html,body{margin:0;background:#000}.clip{position:relative;width:20px;height:20px;overflow-y:clip}.child{position:absolute;left:10px;top:10px;width:30px;height:20px;background:#f00}</style><div class=clip><div class=child></div></div>";
    let cmd = |pairs: &[(&str, &str)], text: &str| {
        Message::new(pairs.iter().copied().collect(), text.as_bytes().to_vec())
    };
    host.handle(&cmd(&[("a", "doc"), ("s", "x")], doc));
    host.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "10"), ("r", "4")],
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    let frame = host.frame("x").unwrap();
    let pixel = |x: u32, y: u32| frame.rgba[((y * frame.width + x) * 4) as usize];
    assert_eq!(
        pixel(35, 15),
        255,
        "horizontal overflow should remain visible"
    );
    assert_eq!(pixel(15, 25), 0, "vertical overflow should be clipped");
}

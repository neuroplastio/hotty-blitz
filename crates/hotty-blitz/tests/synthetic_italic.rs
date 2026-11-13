//! Synthetic italic glyphs lean right in device coordinates.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

#[test]
fn synthetic_italic_leans_right() {
    let font = STANDARD.encode(include_bytes!("../fonts/IBMPlexSans-Regular.woff2"));
    let html = format!(
        "<style>@font-face{{font-family:single;src:url(data:font/woff2;base64,{font});font-style:normal}}\
         html,body{{margin:0;background:#000}}\
         div{{font-family:single;font-size:80px;font-style:italic;color:#fff;line-height:1}}</style><div>H</div>"
    );
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
    host.handle(&cmd(&[("a", "doc"), ("s", "x")], &html));
    host.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "20"), ("r", "6")],
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    let frame = host.frame("x").unwrap();
    let mut pixels = Vec::new();
    for (i, rgba) in frame.rgba.chunks_exact(4).enumerate() {
        if rgba[0] > 128 {
            pixels.push((
                (i as u32 % frame.width) as f64,
                (i as u32 / frame.width) as f64,
            ));
        }
    }
    assert!(!pixels.is_empty(), "the glyph must be visible");
    let y0 = pixels.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let y1 = pixels.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let centroid = |top: bool| {
        let points: Vec<_> = pixels
            .iter()
            .filter(|p| {
                if top {
                    p.1 < y0 + (y1 - y0) / 4.0
                } else {
                    p.1 > y1 - (y1 - y0) / 4.0
                }
            })
            .collect();
        points.iter().map(|p| p.0).sum::<f64>() / points.len() as f64
    };
    let (top, bottom) = (centroid(true), centroid(false));
    assert!(
        top > bottom + 2.0,
        "top centroid {top} should be right of bottom centroid {bottom}"
    );
}

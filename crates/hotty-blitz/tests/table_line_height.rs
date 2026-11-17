//! A forced line break in a table cell occupies two line boxes.
use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};
fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}
#[test]
fn two_lines_with_a_break_take_two_line_heights() {
    for align in ["middle", "baseline", "top", "bottom"] {
        let mut h = Host::new(Config {
            metrics: Metrics {
                cell_w: 10,
                cell_h: 20,
                scale: 1.0,
            },
            ..Config::default()
        });
        let html = format!(
            "<style>body{{margin:0;background:#000;font:14px/20px 'IBM Plex Sans'}}table{{border-spacing:0}}td{{padding:4px 8px;background:#f00;vertical-align:{align}}}</style><table><tr><td>cell<br>two lines</td></tr></table>"
        );
        h.handle(&cmd(&[("a", "doc"), ("s", "x")], &html));
        h.handle(&cmd(
            &[("a", "place"), ("s", "x"), ("c", "30"), ("r", "8")],
            "",
        ));
        h.render_dirty(&mut |_, _, _| {});
        let f = h.frame("x").unwrap();
        let height = (0..f.height)
            .filter(|&y| {
                let i = ((y * f.width + 2) * 4) as usize;
                f.rgba[i..i + 3] == [255, 0, 0]
            })
            .count();
        assert_eq!(
            height, 48,
            "vertical-align: {align}: two 20px lines plus 8px padding"
        );
    }
}

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn render(filter: &str) -> (u32, Vec<u8>) {
    let mut host = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    let cmd = |pairs: &[(&str, &str)], payload: &str| {
        Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
    };
    let html = format!(
        "<style>body{{margin:0;background:white}}div{{position:absolute;left:40px;top:30px;width:40px;height:40px;background:red;filter:{filter}}}</style><div></div>"
    );
    host.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], &html));
    // Above the small-context threshold, so the normal path uses workers.
    host.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "80"),
            ("r", "30"),
            ("q", "2"),
        ],
        "",
    ));
    host.render_dirty(&mut |_, _, _| {});
    let frame = host.frame("x").unwrap();
    (frame.width, frame.rgba.clone())
}

fn pixel(frame: &(u32, Vec<u8>), x: u32, y: u32) -> [u8; 3] {
    let i = ((y * frame.0 + x) * 4) as usize;
    [frame.1[i], frame.1[i + 1], frame.1[i + 2]]
}

#[test]
fn invert_changes_the_element_color() {
    let frame = render("invert(1)");
    assert_eq!(pixel(&frame, 60, 50), [0, 255, 255]);
}

#[test]
fn brightness_changes_the_element_color() {
    let frame = render("brightness(0)");
    assert_eq!(pixel(&frame, 60, 50), [0, 0, 0]);
}

#[test]
fn adjacent_color_matrices_apply_in_order() {
    let frame = render("brightness(.5) invert(1)");
    let p = pixel(&frame, 60, 50);
    assert!(
        (126..=129).contains(&p[0]) && p[1..] == [255, 255],
        "composed color: {p:?}"
    );
}

#[test]
fn color_matrix_chain_clamps_between_passes() {
    let frame = render("brightness(2) contrast(.5)");
    let p = pixel(&frame, 60, 50);
    assert!(
        (190..=193).contains(&p[0]) && (63..=65).contains(&p[1]) && (63..=65).contains(&p[2]),
        "clamped color: {p:?}"
    );
}

#[test]
fn blur_softens_the_element_edge() {
    let frame = render("blur(4px)");
    let edge = pixel(&frame, 40, 50);
    assert!(edge[1] > 0 && edge[1] < 255, "blurred edge: {edge:?}");
}

#[test]
fn drop_shadow_paints_outside_the_element() {
    let frame = render("drop-shadow(12px 0 0 black)");
    assert_eq!(pixel(&frame, 88, 50), [0, 0, 0]);
}

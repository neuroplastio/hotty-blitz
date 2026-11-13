//! A box-shadow's blur is CSS's: a Gaussian whose standard deviation is
//! half the blur radius, at the device scale (mods/blitz/shadow-blur-width.md).

use hotty::Message;
use hotty_blitz::{Config, Host, Metrics};

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

/// erfc, to 1.5e-7 (Abramowitz and Stegun 7.1.26).
fn erfc(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.3275911 * x.abs());
    let p = t
        * (0.254829592
            + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    let e = p * (-x * x).exp();
    if x >= 0.0 { e } else { 2.0 - e }
}

/// A 300 px square box at 100,100 CSS px on a white page, 600×500 CSS px
/// drawn at `scale`: the frame's luma on the row through the box's middle.
fn row(scale: f32, style: &str) -> Vec<u8> {
    let html = format!(
        "<html><style>html,body{{margin:0;background:#fff}}
        .b{{position:absolute;left:100px;top:100px;width:300px;height:300px;{style}}}
        </style><div class=b></div></html>"
    );
    let (cw, ch) = ((10.0 * scale) as u32, (20.0 * scale) as u32);
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: cw,
            cell_h: ch,
            scale,
        },
        ..Config::default()
    });
    h.handle(&cmd(&[("a", "doc"), ("s", "x"), ("q", "2")], &html));
    h.handle(&cmd(
        &[
            ("a", "place"),
            ("s", "x"),
            ("c", "60"),
            ("r", "25"),
            ("q", "2"),
        ],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    let f = h.frame("x").unwrap();
    let (w, y) = (f.width as usize, (250.0 * scale) as usize);
    f.rgba[y * w * 4..(y + 1) * w * 4]
        .chunks(4)
        .map(|p| p[1])
        .collect()
}

/// The shadow's alpha on `px`, the row through the box, at pixels whose
/// centres are σ/4 to 2σ from the box's edge at `edge` device pixels, going
/// `dir` (1 out of a right edge, -1 into it), against a Gaussian of σ = 12
/// CSS px, a blur of 24px: within 0.01.
fn fades_as_css(scale: f32, px: &[u8], edge: f64, dir: f64) {
    let sigma = 12.0 * scale as f64;
    for k in 1..=8 {
        let x = (edge + dir * sigma * k as f64 / 4.0 - 0.5).round();
        let d = dir * (x + 0.5 - edge);
        let want = 0.5 * erfc(d / (sigma * std::f64::consts::SQRT_2));
        let got = 1.0 - px[x as usize] as f64 / 255.0;
        assert!(
            (got - want).abs() < 0.01,
            "scale {scale}: alpha {got:.3} at {d:.1} px, CSS's {want:.3}"
        );
    }
}

/// `0 0 24px #000` on a black box, outside its right edge. Blitz drew
/// σ = 24/√2 device pixels at any scale: 1.41 times CSS's at 1, 0.88 times
/// at 1.6.
#[test]
fn an_outset_shadow_is_css_s_gaussian() {
    for scale in [1.0f32, 1.6] {
        let px = row(scale, "background:#000;box-shadow:0 0 24px #000");
        fades_as_css(scale, &px, 400.0 * scale as f64, 1.0);
    }
}

/// `inset 0 0 24px #000` in a white box, inside its right edge. Blitz drew
/// σ = 24/√2 CSS px: 1.41 times CSS's at any scale.
#[test]
fn an_inset_shadow_is_css_s_gaussian() {
    for scale in [1.0f32, 1.6] {
        let px = row(scale, "background:#fff;box-shadow:inset 0 0 24px #000");
        fades_as_css(scale, &px, 400.0 * scale as f64, -1.0);
    }
}

//! A glyph run that draws nothing is left out of the scene (paint.rs,
//! `Inked`): a document paints the same pixels with it as without it.

use anyrender::ImageRenderer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use hotty_blitz::paint::{Ink, Inked};

const W: u32 = 320;
const H: u32 = 160;

fn pixels(doc: &mut blitz_dom::BaseDocument, ink: Option<&mut Ink>) -> (Vec<u8>, u32) {
    let mut renderer = VelloCpuImageRenderer::new(W, H);
    let mut out = vec![0u8; (W * H * 4) as usize];
    let mut left_out = 0;
    renderer.render(
        |scene| match ink {
            Some(ink) => {
                let mut scene = Inked::new(scene, ink);
                blitz_paint::paint_scene(&mut scene, doc, 2.0, W, H, 0, 0);
                left_out = scene.left_out;
            }
            None => blitz_paint::paint_scene(scene, doc, 2.0, W, H, 0, 0),
        },
        &mut out,
    );
    (out, left_out)
}

#[test]
fn runs_of_spaces_are_left_out_and_nothing_changes() {
    // Cells of one space each (plx's boxes), spaces between words, an
    // underlined run of spaces (its line is drawn apart from the glyphs),
    // spaced letters, a clip after an even-odd border, and outlines.
    let html = r#"<!doctype html><style>
        body { margin: 0; background: #fff; color: #123; font: 12px/16px monospace }
        .r { white-space: pre; height: 16px; overflow: hidden }
        .r i { display: inline-block; width: 8px; height: 16px; overflow: hidden; font-style: normal }
        .r span { display: inline-block; background: #9cf; color: #9cf }
        u { text-decoration: underline 2px #c00 }
        .sp { letter-spacing: 3px }
        .b { border: 3px double #393; border-radius: 6px; width: 60px; height: 20px; overflow: hidden }
        .b p { margin: 0; clip-path: polygon(0 0, 100% 0, 0 100%) }
        </style>
        <div class="r"><i> </i><span><i> </i><i> </i><i>x</i><i> </i></span><i> </i></div>
        <div class="r"><i>│</i><i> </i><i>&nbsp;</i><i>─</i></div>
        <p>two words&nbsp;and <u>   </u> more   spaces</p>
        <p class="sp">s p a c e d</p>
        <div class="b"><p>clipped text</p></div>
        "#;
    let mut doc = HtmlDocument::from_html(
        html,
        DocumentConfig {
            viewport: Some(Viewport::new(W, H, 2.0, ColorScheme::Light)),
            font_ctx: Some(hotty_blitz::fonts::context()),
            ..Default::default()
        },
    );
    doc.resolve(0.0);
    let (plain, _) = pixels(&mut doc, None);
    let mut ink = Ink::default();
    let (inked, left_out) = pixels(&mut doc, Some(&mut ink));
    assert!(left_out >= 6, "runs of spaces left out: {left_out}");
    assert!(plain.chunks_exact(4).any(|p| p[0] < 128), "the text drew");
    let differing = plain.iter().zip(&inked).filter(|(a, b)| a != b).count();
    assert_eq!(differing, 0, "bytes that differ");
    // Again, the map warm: the same.
    let (again, _) = pixels(&mut doc, Some(&mut ink));
    assert!(again == plain);
}

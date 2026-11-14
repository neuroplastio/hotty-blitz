//! A box-shadow change needs repaint, not a box rebuild.

use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;

#[test]
fn changing_box_shadow_does_not_rebuild_the_box() {
    let mut doc = HtmlDocument::from_html(
        "<div id='a' style='box-shadow:0 0 1px red'></div><div id='b' style='box-shadow:0 0 4px red'></div>",
        DocumentConfig::default(),
    );
    doc.resolve(0.0);
    let a = doc
        .get_node(doc.get_element_by_id("a").unwrap())
        .unwrap()
        .primary_styles()
        .unwrap();
    let b = doc
        .get_node(doc.get_element_by_id("b").unwrap())
        .unwrap()
        .primary_styles()
        .unwrap();
    let damage = style::servo::restyle_damage::compute_damage(&a, &b);
    assert_eq!(
        damage,
        style::servo::restyle_damage::ServoRestyleDamage::REPAINT,
        "box-shadow should repaint without layout damage"
    );
}

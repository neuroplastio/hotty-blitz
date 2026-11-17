//! An unrelated :has() rule should not force a full repaint on each delta.
mod common;

use common::delta_cmd;
use hotty::Message;
use hotty_blitz::{Config, Damage, Host, Metrics};

fn cmd(pairs: &[(&str, &str)], payload: &str) -> Message {
    Message::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
}

#[test]
fn unrelated_class_change_keeps_damage_partial() {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], "<style>body{margin:0;background:black}.item{width:100px;height:30px;background:red}.item.active{background:lime} #other:has(.special), #other:has([type=checkbox]){color:red}</style><div id=item class=item type=button></div><div id=other></div>"));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "80"), ("r", "40")],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    let before = h.frame("x").unwrap().rgba.clone();
    h.handle(&delta_cmd(
        &[("s", "x")],
        "attr",
        Some("item"),
        Some("class"),
        "item active",
    ));
    let mut full = false;
    h.render_dirty(&mut |_, _, d| full |= matches!(d, Damage::Full));
    let partial = h.frame("x").unwrap().rgba.clone();
    assert!(before != partial, "the item changed");
    assert!(!full, "an unrelated :has() forced a full repaint");
    h.repaint();
    h.render_dirty(&mut |_, _, _| {});
    assert!(
        partial
            .iter()
            .zip(&h.frame("x").unwrap().rgba)
            .all(|(a, b)| a.abs_diff(*b) <= 1),
        "partial repaint differs from full"
    );
}

#[test]
fn multiple_attribute_changes_still_invalidate_relative_selectors() {
    let mut h = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    h.handle(&cmd(&[("a", "doc"), ("s", "x")], "<style>body{margin:0} #box{width:80px;height:40px;background:red} #box:has(#target.on[data-flag=yes]){background:lime}</style><div id=box><div><span id=target></span></div></div>"));
    h.handle(&cmd(
        &[("a", "place"), ("s", "x"), ("c", "20"), ("r", "5")],
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    let f = h.frame("x").unwrap();
    let p = ((20 * f.width + 60) * 4) as usize;
    assert_eq!(&f.rgba[p..p + 3], &[255, 0, 0], "initial color");
    for (name, value) in [("class", "on"), ("data-flag", "yes")] {
        h.handle(&delta_cmd(
            &[("s", "x")],
            "attr",
            Some("target"),
            Some(name),
            value,
        ));
    }
    h.render_dirty(&mut |_, _, _| {});
    let f = h.frame("x").unwrap();
    assert_eq!(
        &f.rgba[p..p + 3],
        &[0, 255, 0],
        "both changes restyled the ancestor"
    );
    h.handle(&delta_cmd(
        &[("s", "x")],
        "unattr",
        Some("target"),
        Some("data-flag"),
        "",
    ));
    h.render_dirty(&mut |_, _, _| {});
    let f = h.frame("x").unwrap();
    assert_eq!(
        &f.rgba[p..p + 3],
        &[255, 0, 0],
        "removing the attribute restyled the ancestor"
    );
}

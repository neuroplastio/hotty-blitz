//! Deltas after which a node they freed was still named: by a morph, or by
//! Blitz's layout data. Each panicked ("invalid SlotMap key used", or an
//! unwrap in paint), at the delta or at the render after it; hottyterm then
//! turned HOTTY off. Found by fuzzing random deltas against layout, and
//! shrunk.

use hotty_blitz::{Config, Host, Metrics};
use hotty_wire::Command;

enum Step<'a> {
    Delta(&'a str, &'a str, Option<&'a str>, &'a str),
    Render,
}
use Step::*;

fn run(doc: &str, steps: &[Step]) {
    let mut host = Host::new(Config {
        metrics: Metrics {
            cell_w: 10,
            cell_h: 20,
            scale: 1.0,
        },
        ..Config::default()
    });
    let cmd = |pairs: &[(&str, &str)], payload: &str| {
        Command::new(pairs.iter().copied().collect(), payload.as_bytes().to_vec())
    };
    host.handle(&cmd(&[("a", "doc"), ("s", "f"), ("q", "2")], doc));
    host.handle(&cmd(&[("a", "place"), ("s", "f"), ("c", "40"), ("r", "10"), ("q", "2")], ""));
    host.render_dirty(&mut |_, _, _| {});
    for step in steps {
        match step {
            Delta(op, t, k, payload) => {
                let mut pairs = vec![("a", "delta"), ("s", "f"), ("op", *op), ("t", *t), ("q", "2")];
                pairs.extend(k.map(|k| ("k", k)));
                host.handle(&cmd(&pairs, payload));
            }
            Render => host.render_dirty(&mut |_, _, _| {}),
        }
    }
    host.render_dirty(&mut |_, _, _| {});
}

/// An id moved to another tag: the morph replaced the element, freeing
/// it, then put the freed one in order among its siblings.
#[test]
fn an_id_moved_to_another_tag() {
    run(
        "<div id=list><div id=x>a</div><p>b</p></div>",
        &[Delta("inner", "list", None, "<p>b</p><span id=x>a</span>")],
    );
}

/// One delta that builds a form control and frees it again (an id given
/// twice, on two tags: the second morph replaces what the first built):
/// Blitz's mutator had queued the control's form owner, and found it freed
/// when it flushed.
#[test]
fn a_control_built_and_freed_by_one_delta() {
    run(
        "<section id=s><span id=x></span> </section>",
        &[Delta("prepend", "s", None, "<button id=x></button><a id=x> </a>")],
    );
}

/// A table keeps the rows it was built with until it is built again, and
/// construction skipped a node with no children: paint read the rows.
#[test]
fn a_table_emptied() {
    run(
        "<label id=l></label>",
        &[
            Delta("replace", "l", None, r#"<b id=t style="display:table"><ul style="display:table-row">x</ul></b>"#),
            Render,
            Delta("inner", "t", None, ""),
        ],
    );
}

/// A positioned box that goes `display: none` keeps the boxes it hoisted
/// as their containing block; Taffy's rounding still visits them, and
/// what was removed under them since.
#[test]
fn a_containing_block_hidden_as_a_box_under_it_goes() {
    run(
        r#"<em id=cb style="position:absolute"><p id=abs style="position:absolute"></p></em>text<div>x</div>"#,
        &[
            Delta("inner", "abs", None, "<div id=gone></div>"),
            Render,
            Delta("attr", "cb", Some("hidden"), ""),
            Delta("remove", "gone", None, ""),
        ],
    );
}

/// The same with a fixed box, its hoisted box removed with its children.
#[test]
fn a_fixed_box_hidden_and_emptied() {
    run(
        "<b id=f> </b> <p></p>",
        &[
            Delta("morph", "f", None, r#"<section id=f style="position:fixed"><div id=d> </div></section>"#),
            Delta("inner", "d", None, r#"<ul style="position:absolute"></ul>x"#),
            Render,
            Delta("attr", "f", Some("hidden"), ""),
            Delta("inner", "f", None, ""),
        ],
    );
}

/// A box that goes `display: none` is not constructed again (its parent
/// is), and its subtree, unstyled, reports no removal: it kept layout
/// children that named freed nodes, which rounding visited.
#[test]
fn a_box_hidden_as_its_grandchildren_go() {
    run(
        "<em id=h><li id=li><b id=b></b></li>x</em>x",
        &[
            Delta("before", "b", None, "<ul> </ul>"),
            Render,
            Delta("inner", "li", None, ""),
            Delta("attr", "h", Some("style"), "display:none"),
            Render,
            Delta("after", "h", None, "<ul></ul>"),
        ],
    );
}

/// The same with an inline box: the hidden box kept its inline layout,
/// laid out when its parent was constructed again.
#[test]
fn a_box_hidden_as_its_inline_child_goes() {
    run(
        r#"<li id=first style="display:inline-flex"></li><div id=h>  <li id=gone style="display:inline-flex"></li></div>"#,
        &[
            Delta("attr", "h", Some("style"), "display:none"),
            Delta("replace", "gone", None, ""),
            Render,
            Delta("before", "first", None, r#"<a style="display:table-cell"></a> "#),
        ],
    );
}

/// Speculative relayout asked a node out of the layout tree (here one
/// gone `display: contents`) about its layout parent, an anonymous block
/// freed since.
#[test]
fn a_flex_item_gone_display_contents() {
    run(
        "<button id=b></button> ",
        &[
            Delta("append", "b", None, r#"<section><div> </div><p id=p style="display:inline-flex"></p></section>"#),
            Render,
            Delta("attr", "p", Some("style"), "display:contents"),
            Delta("prepend", "p", None, ""),
            Render,
        ],
    );
}

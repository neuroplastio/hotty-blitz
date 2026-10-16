//! The menu a program shows with `a=menu` (HOTTY SPEC §10.5), drawn as a
//! surface of the host's own: myelin's which-key level, as a document.

use crate::body::{Menu, MenuItem};
use unicode_segmentation::UnicodeSegmentation;

/// The surface the host draws the menu in. A program's surface name never
/// has a `:` (SPEC §5).
pub const MENU: &str = "hotty:menu";

/// The longest label a menu shows, in cells; the rest is cut.
const LABEL_CELLS: usize = 32;
/// The most items a column of the menu holds.
const COLUMN_ITEMS: usize = 10;

/// A menu as a document: a title row, the keys typed and their group's
/// label, then each key that can follow with what it does, in columns of
/// at most [`COLUMN_ITEMS`]. A group's label has a `+`, and a disabled
/// item is dimmed. `#m` is as wide and as tall as what it shows, which
/// the host rounds up to cells; the whole surface has its background.
pub(crate) fn menu_html(m: &Menu) -> String {
    let shown = |i: &MenuItem| {
        let mut label = if i.group && !i.label.starts_with('+') {
            format!("+{}", i.label)
        } else {
            i.label.clone()
        };
        if label.graphemes(true).count() > LABEL_CELLS {
            label = label
                .graphemes(true)
                .take(LABEL_CELLS - 1)
                .collect::<String>()
                + "…";
        }
        label
    };
    let per = m
        .items
        .len()
        .div_ceil(m.items.len().div_ceil(COLUMN_ITEMS).max(1))
        .max(1);
    let mut html = String::from(concat!(
        "<style>",
        "html{background:color-mix(in srgb,var(--hotty-fg) 10%,var(--hotty-bg))}",
        "body{margin:0}",
        "#m{display:inline-block;padding:0 var(--hotty-cell-w);color:var(--hotty-fg);",
        "line-height:var(--hotty-cell-h);white-space:pre}",
        ".s{color:var(--hotty-accent)}",
        ".l{display:flex;gap:calc(2*var(--hotty-cell-w))}",
        ".i{height:var(--hotty-cell-h)}",
        ".k{color:var(--hotty-accent);font-weight:bold}",
        ".g{color:var(--hotty-ansi-5)}",
        ".off{opacity:.5}",
        "</style>",
    ));
    let title = [m.seq.join(" "), m.label.clone()];
    html.push_str(&format!(
        "<div id=m><div class=i><span class=s>{}</span>{}{}</div><div class=l>",
        escape(&title[0]),
        if title[1].is_empty() { "" } else { " " },
        escape(&title[1]),
    ));
    for col in m.items.chunks(per) {
        html.push_str("<div>");
        for i in col {
            let class = match (i.group, i.enabled) {
                (_, false) => "i off",
                (true, true) => "i g",
                _ => "i",
            };
            html.push_str(&format!(
                "<div class=\"{class}\"><span class=k>{}</span> {}</div>",
                escape(&i.key),
                escape(&shown(i)),
            ));
        }
        html.push_str("</div>");
    }
    html.push_str("</div></div>");
    html
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

//! The menu a program keeps with `a=menu` (hotty SPEC §10.7), drawn as a
//! surface of the host's own: myelin's which-key level, as a document.

use crate::body::{Menu, MenuItem};
use hotty_wire::keys::{self, Platform};
use unicode_segmentation::UnicodeSegmentation;

/// The surface the host draws the level shown in. A program's surface name
/// never has a `:` (SPEC §5).
pub const MENU: &str = "hotty:menu";

/// The longest label a menu shows, in cells; the rest is cut.
const LABEL_CELLS: usize = 32;
/// The most items a column of the level holds.
const COLUMN_ITEMS: usize = 10;

/// An item of the level shown (SPEC §10.7): its key and label as the
/// program sent them, whether keys follow it, and whether it is enabled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    pub key: String,
    pub label: String,
    pub group: bool,
    pub enabled: bool,
}

/// The level of a menu the keys typed reach, as the host shows it: the
/// keys, what they reach is called, and the items that have a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Level {
    pub seq: Vec<String>,
    pub label: String,
    pub items: Vec<Shown>,
}

/// The level `seq` reaches in `menu`. Each key picks the first group at the
/// level that it names, read as a binding's key is (SPEC §10.4). None when
/// the keys reach no group, or the level has no item with a key.
pub(crate) fn level(menu: &Menu, seq: &[String], p: &Platform) -> Option<Level> {
    let mut label = &menu.label;
    let mut items = &menu.items;
    for typed in seq {
        let typed = keys::read_key(p, typed)?;
        let group = items
            .iter()
            .find(|i| i.group() && keys::read_key(p, &i.key).as_ref() == Some(&typed))?;
        label = &group.label;
        items = &group.items;
    }
    let items: Vec<Shown> = items
        .iter()
        .filter(|i| !i.key.is_empty())
        .map(|i| Shown {
            key: i.key.clone(),
            label: i.label.clone(),
            group: i.group(),
            enabled: i.enabled,
        })
        .collect();
    (!items.is_empty()).then(|| Level {
        seq: seq.to_vec(),
        label: label.clone(),
        items,
    })
}

/// The first item named `name`, depth-first (SPEC §10.7).
pub(crate) fn find<'a>(items: &'a [MenuItem], name: &str) -> Option<&'a MenuItem> {
    items.iter().find_map(|i| {
        if i.name == name {
            Some(i)
        } else {
            find(&i.items, name)
        }
    })
}

/// A key as the host shows it: `mod` read as the host's `mod` says, and
/// shown as myelin key names §6 has it, canonical but for the arrows, `tab`
/// and `shift+tab`.
fn shown_key(p: &Platform, key: &str) -> String {
    let Some(read) = keys::read_key(p, key) else {
        return key.to_string();
    };
    let k = keys::KeyParts::split(&read);
    let (value, shift) = match k.value.as_str() {
        "arrow_left" => ("←", true),
        "arrow_right" => ("→", true),
        "arrow_up" => ("↑", true),
        "arrow_down" => ("↓", true),
        "tab" if k.shift() => ("⇧⇥", false),
        "tab" => ("⇥", true),
        v => (v, true),
    };
    let mods = if shift { k.mods } else { k.mods & !keys::SHIFT };
    let mut out = keys::KeyParts { mods, value: String::new() }.name();
    out.push_str(value);
    out
}

/// A level as a document: a title row, the keys typed and their group's
/// label, then each key that can follow with what it does, in columns of
/// at most [`COLUMN_ITEMS`]. A group's label has a `+`, and a disabled
/// item is dimmed. `#m` is as wide and as tall as what it shows, which
/// the host rounds up to cells; the whole surface has its background.
pub(crate) fn menu_html(m: &Level, p: &Platform) -> String {
    let shown = |i: &Shown| {
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
    let title = [
        m.seq
            .iter()
            .map(|k| shown_key(p, k))
            .collect::<Vec<_>>()
            .join(" "),
        m.label.clone(),
    ];
    html.push_str(&format!(
        "<div id=m><div class=i><span class=s>{}</span>{}{}</div><div class=l>",
        escape(&title[0]),
        if title[0].is_empty() || title[1].is_empty() { "" } else { " " },
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
                escape(&shown_key(p, &i.key)),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A key is shown canonical, but for the arrows and tab (myelin key
    /// names §6), `mod` read as the host's.
    #[test]
    fn keys_are_shown_as_myelin_key_names_display_them() {
        let ctrl = Platform::default();
        let sup = Platform {
            mod_super: true,
            edit: None,
        };
        for (key, p, want) in [
            ("g", &ctrl, "g"),
            ("Space", &ctrl, "space"),
            ("Left", &ctrl, "←"),
            ("arrow_right", &ctrl, "→"),
            ("up", &ctrl, "↑"),
            ("arrow_down", &ctrl, "↓"),
            ("tab", &ctrl, "⇥"),
            ("shift+tab", &ctrl, "⇧⇥"),
            ("ctrl+shift+tab", &ctrl, "ctrl+⇧⇥"),
            ("shift+mod+arrow_left", &ctrl, "ctrl+shift+←"),
            ("mod+x", &sup, "super+x"),
            ("meta+x", &sup, "meta+x"),
        ] {
            assert_eq!(shown_key(p, key), want, "{key}");
        }
    }
}

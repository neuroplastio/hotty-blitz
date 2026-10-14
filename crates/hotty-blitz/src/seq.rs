//! Key sequences (HOTTY SPEC §10.5): the elements with a `data-key`, which
//! of them the keys typed reach, as myelin's rules have it, and the
//! which-key menu of the keys that can follow them.

use crate::body::{Menu, MenuItem};
use blitz_dom::{BaseDocument, NodeId};
use hotty_wire::keys::{self, Platform};
use unicode_segmentation::UnicodeSegmentation;

/// The surface the host draws its which-key menu in. A program's surface
/// name never has a `:` (SPEC §5).
pub const MENU: &str = "hotty:menu";

/// An element's sequence: the keys of the nearest element around it with
/// one, then its own.
pub(crate) struct Seq {
    pub id: NodeId,
    pub keys: Vec<String>,
    /// Elements with sequences are inside it: its sequence leads to theirs,
    /// and does nothing itself.
    pub group: bool,
}

/// The elements with a `data-key` a host reads, in tree order, each with
/// its whole sequence. A value the host ignores is as if absent: the
/// elements inside it go on from the one around it.
pub(crate) fn sequences(doc: &BaseDocument, platform: &Platform, leader: &str) -> Vec<Seq> {
    let mut out: Vec<Seq> = Vec::new();
    // Each node to visit, with the index in `out` of its nearest keyed
    // ancestor.
    let mut stack = vec![(doc.root_node().id, None::<usize>)];
    while let Some((id, around)) = stack.pop() {
        let Some(node) = doc.get_node(id) else {
            continue;
        };
        let mut inner = around;
        let value = node
            .attrs()
            .and_then(|a| a.iter().find(|a| &*a.name.local == "data-key"))
            .map(|a| a.value.to_string());
        if let Some(own) = value.and_then(|v| keys::parse_sequence(platform, leader, &v)) {
            let mut seq = around.map(|i| out[i].keys.clone()).unwrap_or_default();
            seq.extend(own);
            if let Some(i) = around {
                out[i].group = true;
            }
            out.push(Seq {
                id,
                keys: seq,
                group: false,
            });
            inner = Some(out.len() - 1);
        }
        stack.extend(node.children.iter().rev().map(|&c| (c, inner)));
    }
    out
}

/// How near the focused element each element is (SPEC §10.5): the depth of
/// its nearest common ancestor with it, the root's 0. Without a focused
/// element, every element is as near as any.
pub(crate) struct Nearness {
    /// The focused element and its ancestors, the root last.
    chain: Vec<NodeId>,
}

impl Nearness {
    pub fn new(doc: &BaseDocument, focused: Option<NodeId>) -> Nearness {
        let mut chain = Vec::new();
        let mut n = focused;
        while let Some(node) = n.and_then(|id| doc.get_node(id)) {
            chain.push(node.id);
            n = node.parent;
        }
        Nearness { chain }
    }

    pub fn of(&self, doc: &BaseDocument, id: NodeId) -> usize {
        let mut n = Some(id);
        while let Some(node) = n.and_then(|id| doc.get_node(id)) {
            if let Some(i) = self.chain.iter().position(|&c| c == node.id) {
                return self.chain.len() - 1 - i;
            }
            n = node.parent;
        }
        0
    }
}

/// What the keys typed do (SPEC §10.5).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// They finish the sequence of this element, which runs.
    Run(NodeId),
    /// A longer sequence goes on from them: they wait for the next key.
    Wait,
    /// No sequence finishes or goes on from them.
    Dead,
}

/// The element whose sequence `typed` finishes, the one nearest the focus
/// and of those the last in tree order; and whether a longer sequence at
/// least as near goes on from it, or, with none finished, any does.
pub(crate) fn step(doc: &BaseDocument, seqs: &[Seq], near: &Nearness, typed: &[String]) -> Step {
    let leaves = || seqs.iter().filter(|s| !s.group);
    let finished = leaves()
        .filter(|s| s.keys == typed)
        .map(|s| (near.of(doc, s.id), s.id))
        // The last of the nearest: max_by_key keeps the last of equals.
        .max_by_key(|&(n, _)| n);
    let mut longer = leaves()
        .filter(|s| s.keys.len() > typed.len() && s.keys.starts_with(typed))
        .map(|s| near.of(doc, s.id));
    match finished {
        Some((n, id)) => {
            if longer.any(|l| l >= n) {
                Step::Wait
            } else {
                Step::Run(id)
            }
        }
        None if longer.next().is_some() => Step::Wait,
        None => Step::Dead,
    }
}

/// The element whose sequence `typed` finishes, when a wait times out.
pub(crate) fn finished(
    doc: &BaseDocument,
    seqs: &[Seq],
    near: &Nearness,
    typed: &[String],
) -> Option<NodeId> {
    seqs.iter()
        .filter(|s| !s.group && s.keys == typed)
        .map(|s| (near.of(doc, s.id), s.id))
        .max_by_key(|&(n, _)| n)
        .map(|(_, id)| id)
}

/// The which-key menu while `typed` waits: each key that can follow it, in
/// tree order, labelled by the element its sequence there finishes (the
/// nearest), or by its group's `aria-label`.
pub(crate) fn menu(doc: &BaseDocument, seqs: &[Seq], near: &Nearness, typed: &[String]) -> Menu {
    let n = typed.len();
    let label = seqs
        .iter()
        .filter(|s| s.group && s.keys == typed)
        .find_map(|s| aria_label(doc, s.id))
        .unwrap_or_default();
    let mut items: Vec<MenuItem> = Vec::new();
    for s in seqs
        .iter()
        .filter(|s| !s.group && s.keys.len() > n && s.keys.starts_with(typed))
    {
        let key = &s.keys[n];
        if items.iter().any(|i| &i.key == key) {
            continue;
        }
        let next: Vec<String> = s.keys[..=n].to_vec();
        let under: Vec<&Seq> = seqs
            .iter()
            .filter(|t| !t.group && t.keys.starts_with(&next))
            .collect();
        let group = under.iter().any(|t| t.keys.len() > n + 1);
        let own = finished(doc, seqs, near, &next);
        let label = match own {
            Some(id) => element_label(doc, id),
            None => seqs
                .iter()
                .filter(|t| t.group && t.keys == next)
                .find_map(|t| aria_label(doc, t.id))
                .unwrap_or_else(|| format!("+{}", under.len())),
        };
        let enabled = match own {
            Some(id) => !disabled(doc, id),
            None => under.iter().any(|t| !disabled(doc, t.id)),
        };
        items.push(MenuItem {
            key: key.clone(),
            label,
            group,
            enabled,
        });
    }
    Menu {
        seq: typed.to_vec(),
        label,
        items,
    }
}

/// An element's `aria-label`, when it has one that is not blank.
fn aria_label(doc: &BaseDocument, id: NodeId) -> Option<String> {
    let v = doc
        .get_node(id)?
        .attrs()?
        .iter()
        .find(|a| &*a.name.local == "aria-label")
        .map(|a| collapse(&a.value))?;
    (!v.is_empty()).then_some(v)
}

/// What a menu calls an element: its `aria-label`, or else its text, with
/// its white space collapsed.
fn element_label(doc: &BaseDocument, id: NodeId) -> String {
    aria_label(doc, id).unwrap_or_else(|| {
        doc.get_node(id)
            .map(|n| collapse(&n.text_content()))
            .unwrap_or_default()
    })
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether an element is disabled: its sequence does nothing.
pub(crate) fn disabled(doc: &BaseDocument, id: NodeId) -> bool {
    doc.get_node(id)
        .and_then(|n| n.element_data())
        .is_some_and(|e| e.is_disabled())
}

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

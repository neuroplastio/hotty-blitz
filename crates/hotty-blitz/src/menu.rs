//! The menu a program keeps with `a=menu` (HOTTY SPEC §10.7), drawn as
//! surfaces of the host's own: myelin's which-key level, and the palette
//! the user picks from, as documents.

use crate::body::{Menu, MenuItem};
use hotty_wire::keys::{self, Platform};
use unicode_segmentation::UnicodeSegmentation;

/// The surface the host draws the level shown in. A program's surface name
/// never has a `:` (SPEC §5).
pub const MENU: &str = "hotty:menu";
/// The surface the host draws the palette in.
pub const PALETTE: &str = "hotty:palette";

/// The longest label a menu shows, in cells; the rest is cut.
const LABEL_CELLS: usize = 32;
/// The most items a column of the level holds.
const COLUMN_ITEMS: usize = 10;
/// How wide the palette is, in cells.
pub const PALETTE_COLS: u16 = 60;
/// The most rows of items the palette shows at once; the selection scrolls
/// into view.
const PALETTE_ROWS: usize = 12;

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

/// A key as the host shows it: `Mod` read as the host's `mod` says.
fn shown_key(p: &Platform, key: &str) -> String {
    keys::read_key(p, key).unwrap_or_else(|| key.to_string())
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

/// An item a user can pick (SPEC §10.7): it has a name. Its label under
/// its groups', the keys that reach it if each item on its path has one,
/// and whether it is enabled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    pub path: Vec<String>,
    pub name: String,
    pub keys: Option<String>,
    pub enabled: bool,
}

impl Row {
    /// What the palette says of it: the groups' labels and its own, joined
    /// by `›`. Its name where it has no label.
    fn text(&self) -> String {
        self.path.join(" › ")
    }

    /// Whether every word of `filter` is in its text or its name, whatever
    /// the case.
    fn matches(&self, filter: &str) -> bool {
        let (text, name) = (self.text().to_lowercase(), self.name.to_lowercase());
        filter
            .to_lowercase()
            .split_whitespace()
            .all(|w| text.contains(w) || name.contains(w))
    }
}

/// The items of `menu` that have a name, depth-first.
pub(crate) fn rows(menu: &Menu, p: &Platform) -> Vec<Row> {
    fn walk(items: &[MenuItem], path: &mut Vec<String>, keys: &mut Option<Vec<String>>, p: &Platform, out: &mut Vec<Row>) {
        for i in items {
            let saved = keys.clone();
            *keys = match keys.take() {
                Some(mut k) if !i.key.is_empty() => {
                    k.push(shown_key(p, &i.key));
                    Some(k)
                }
                _ => None,
            };
            let label = if i.label.is_empty() { &i.name } else { &i.label };
            if !i.name.is_empty() {
                path.push(label.clone());
                out.push(Row {
                    path: path.clone(),
                    name: i.name.clone(),
                    keys: keys.as_ref().map(|k| k.join(" ")),
                    enabled: i.enabled,
                });
                path.pop();
            }
            if i.group() {
                let own = !i.label.is_empty();
                if own {
                    path.push(i.label.clone());
                }
                walk(&i.items, path, keys, p, out);
                if own {
                    path.pop();
                }
            }
            *keys = saved;
        }
    }
    let mut out = Vec::new();
    walk(&menu.items, &mut Vec::new(), &mut Some(Vec::new()), p, &mut out);
    out
}

/// The palette's state (SPEC §10.7): what is typed into its filter, the
/// row selected among the rows that match it, and the first row it shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Palette {
    filter: String,
    sel: Option<usize>,
    top: usize,
}

/// What a key does to the palette.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PaletteKey {
    /// It was used, or nothing came of it.
    Used,
    /// The palette closes.
    Close,
    /// The palette closes, with this pick.
    Pick(String),
}

/// A row of the palette as drawn: its text, its keys, and whether it is
/// enabled and selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteRow {
    pub text: String,
    pub keys: Option<String>,
    pub enabled: bool,
    pub selected: bool,
}

/// The palette as drawn: its title, its filter, and the rows in view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaletteView {
    pub title: String,
    pub filter: String,
    pub rows: Vec<PaletteRow>,
}

impl Palette {
    fn matching<'a>(&self, rows: &'a [Row]) -> Vec<&'a Row> {
        rows.iter().filter(|r| r.matches(&self.filter)).collect()
    }

    /// The selection moves to the first enabled row that matches, and the
    /// rows in view to it.
    fn reset(&mut self, rows: &[Row]) {
        self.sel = self.matching(rows).iter().position(|r| r.enabled);
        self.top = 0;
        self.scroll(rows);
    }

    /// The rows in view hold the selection, and the list does not end
    /// short of them.
    fn scroll(&mut self, rows: &[Row]) {
        let n = self.matching(rows).len();
        self.top = self.top.min(n.saturating_sub(PALETTE_ROWS));
        if let Some(sel) = self.sel {
            if sel < self.top {
                self.top = sel;
            } else if sel >= self.top + PALETTE_ROWS {
                self.top = sel + 1 - PALETTE_ROWS;
            }
        }
    }

    /// The rows changed (the program sent a menu): the filter stays, and
    /// so does the selection when its item is still there.
    pub fn refresh(&mut self, old: &[Row], new: &[Row]) {
        let was = self.sel.and_then(|s| self.matching(old).get(s).map(|r| r.name.clone()));
        self.sel = was
            .and_then(|n| self.matching(new).iter().position(|r| r.enabled && r.name == n))
            .or_else(|| self.matching(new).iter().position(|r| r.enabled));
        self.scroll(new);
    }

    /// A key typed while the palette is open (SPEC §10.7): `name` as §10.4
    /// names it, `text` what it types.
    pub fn key(&mut self, rows: &[Row], name: &str, text: &str) -> PaletteKey {
        match name {
            "Escape" => return PaletteKey::Close,
            "Enter" => {
                let picked = self.sel.and_then(|s| self.matching(rows).get(s).map(|r| (*r).clone()));
                return match picked {
                    Some(r) if r.enabled => PaletteKey::Pick(r.name),
                    _ => PaletteKey::Used,
                };
            }
            "ArrowUp" | "Control+p" => self.step(rows, false),
            "ArrowDown" | "Control+n" => self.step(rows, true),
            "Backspace" => {
                if self.filter.graphemes(true).next_back().is_some() {
                    let keep = self.filter.graphemes(true).count() - 1;
                    self.filter = self.filter.graphemes(true).take(keep).collect();
                    self.reset(rows);
                }
            }
            _ if !text.is_empty() && !text.chars().any(char::is_control) => {
                self.filter.push_str(text);
                self.reset(rows);
            }
            _ => {}
        }
        PaletteKey::Used
    }

    /// The selection moves to the next enabled row that matches, down or
    /// up; it stays where there is none.
    fn step(&mut self, rows: &[Row], down: bool) {
        let m = self.matching(rows);
        let enabled = |i: &usize| m[*i].enabled;
        let next = match (self.sel, down) {
            (Some(s), true) => (s + 1..m.len()).find(enabled),
            (Some(s), false) => (0..s).rev().find(enabled),
            (None, true) => (0..m.len()).find(enabled),
            (None, false) => (0..m.len()).rev().find(enabled),
        };
        self.sel = next.or(self.sel);
        self.scroll(rows);
    }

    /// Every row that matches the filter, as drawn.
    pub fn rows(&self, rows: &[Row]) -> Vec<PaletteRow> {
        self.matching(rows)
            .iter()
            .enumerate()
            .map(|(i, r)| PaletteRow {
                text: r.text(),
                keys: r.keys.clone(),
                enabled: r.enabled,
                selected: self.sel == Some(i),
            })
            .collect()
    }

    /// The palette over `menu`'s rows: the title, the filter and the rows
    /// in view.
    pub fn view(&self, menu: Option<&Menu>, rows: &[Row]) -> PaletteView {
        let title = menu.map_or("", |m| &m.label);
        PaletteView {
            title: if title.is_empty() { "Actions" } else { title }.to_string(),
            filter: self.filter.clone(),
            rows: self
                .rows(rows)
                .into_iter()
                .skip(self.top)
                .take(PALETTE_ROWS)
                .collect(),
        }
    }

    /// A palette just opened, over `rows`.
    pub fn open(rows: &[Row]) -> Palette {
        let mut p = Palette::default();
        p.reset(rows);
        p
    }
}

/// How many rows a palette document is tall, in cells: its title, its
/// filter and its rows, one at least.
pub(crate) fn palette_rows(v: &PaletteView) -> u16 {
    2 + v.rows.len().max(1) as u16
}

/// The palette as a document, [`PALETTE_COLS`] cells wide and
/// [`palette_rows`] tall: a title row, a filter row with its caret, then a
/// row for each item with its text cut to fit and its keys at the right.
/// The selected row is highlighted, a disabled one dimmed.
pub(crate) fn palette_html(v: &PaletteView) -> String {
    let mut html = String::from(concat!(
        "<style>",
        "html{background:color-mix(in srgb,var(--hotty-fg) 10%,var(--hotty-bg))}",
        "body{margin:0}",
        "#p{box-sizing:border-box;width:calc(60*var(--hotty-cell-w));color:var(--hotty-fg);",
        "line-height:var(--hotty-cell-h);white-space:pre}",
        ".t{height:var(--hotty-cell-h);padding:0 var(--hotty-cell-w);font-weight:bold;",
        "color:var(--hotty-accent);overflow:hidden}",
        ".f{height:var(--hotty-cell-h);padding:0 var(--hotty-cell-w);overflow:hidden}",
        ".c{background:var(--hotty-fg);color:var(--hotty-bg)}",
        ".r{display:flex;justify-content:space-between;gap:var(--hotty-cell-w);",
        "height:var(--hotty-cell-h);padding:0 var(--hotty-cell-w)}",
        ".x{flex:1 1 0;min-width:0;overflow:hidden}",
        ".k{flex:none;color:var(--hotty-accent)}",
        ".sel{background:color-mix(in srgb,var(--hotty-accent) 30%,var(--hotty-bg))}",
        ".off{opacity:.5}",
        ".e{padding:0 var(--hotty-cell-w);opacity:.5}",
        "</style>",
    ));
    html.push_str(&format!(
        "<div id=p><div class=t>{}</div><div class=f><span class=k>›</span> {}<span class=c> </span></div>",
        escape(&v.title),
        escape(&v.filter),
    ));
    if v.rows.is_empty() {
        html.push_str("<div class=e>Nothing to pick</div>");
    }
    for r in &v.rows {
        let class = match (r.selected, r.enabled) {
            (true, _) => "r sel",
            (_, false) => "r off",
            _ => "r",
        };
        html.push_str(&format!(
            "<div class=\"{class}\"><span class=x>{}</span><span class=k>{}</span></div>",
            escape(&r.text),
            escape(r.keys.as_deref().unwrap_or("")),
        ));
    }
    html.push_str("</div>");
    html
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

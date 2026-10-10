//! A text field's editing actions (SPEC §10.2), on its value and its
//! selection. Moves by rows are the field's layout's (Blitz's editor), and
//! typing is Blitz's; the rest is here, so that words, lines and a password
//! are as the spec has them, not as an editor's own keys would do them.

use unicode_segmentation::UnicodeSegmentation;

/// A field as an action finds it: its value, and its selection as byte
/// offsets into it (collapsed: the caret).
pub(crate) struct Field<'a> {
    pub value: &'a str,
    pub anchor: usize,
    pub focus: usize,
    pub multiline: bool,
    pub password: bool,
}

/// What an action left: the value, when it changed, and the selection,
/// byte offsets into the value it left: collapsed, the anchor is the caret.
pub(crate) struct Edited {
    pub value: Option<String>,
    pub anchor: usize,
    pub caret: usize,
}

/// Does `action` to the field; with `extend`, a move keeps the selection's
/// anchor and moves the caret from where it is, as Shift does (SPEC §10.2).
/// `None` for an action this does not do: the moves by rows, `newline`,
/// `submit` and `program`, and the multi-line actions in an input.
pub(crate) fn apply(f: &Field, action: &str, extend: bool) -> Option<Edited> {
    let c: Vec<&str> = f.value.graphemes(true).collect();
    let at = |byte: usize| index_of(&c, byte);
    let (lo, hi) = (f.anchor.min(f.focus), f.anchor.max(f.focus));
    let selected = lo != hi;
    let (start, end, p) = (at(lo), at(hi), at(f.focus));
    let n = c.len();
    let line = |p: usize| -> (usize, usize) {
        if !f.multiline {
            return (0, n);
        }
        let (mut s, mut e) = (p, p);
        while s > 0 && !is_break(c[s - 1]) {
            s -= 1;
        }
        while e < n && !is_break(c[e]) {
            e += 1;
        }
        (s, e)
    };
    let word_back = |mut p: usize| {
        if f.password {
            return 0;
        }
        while p > 0 && is_space(c[p - 1]) {
            p -= 1;
        }
        while p > 0 && !is_space(c[p - 1]) {
            p -= 1;
        }
        p
    };
    let word_forward = |mut p: usize| {
        if f.password {
            return n;
        }
        while p < n && is_space(c[p]) {
            p += 1;
        }
        while p < n && !is_space(c[p]) {
            p += 1;
        }
        p
    };
    // A move starts from the selection's start when it goes back, from its
    // end otherwise; char-backward and char-forward stop there. With Shift,
    // it starts from the caret, and the anchor stays.
    let (back, forward) = if selected && !extend {
        (start, end)
    } else {
        (p, p)
    };
    let moved = |to: usize| {
        let caret = byte_of(&c, to);
        Edited {
            value: None,
            anchor: if extend { f.anchor } else { caret },
            caret,
        }
    };
    let delete = |a: usize, b: usize| {
        let (a, b) = if selected { (start, end) } else { (a, b) };
        if a >= b {
            let caret = byte_of(&c, a);
            return Edited {
                value: None,
                anchor: caret,
                caret,
            };
        }
        let before = c[..a].concat();
        let caret = before.len();
        Edited {
            value: Some(before + &c[b..].concat()),
            anchor: caret,
            caret,
        }
    };
    Some(match action {
        "select-all" => Edited {
            value: None,
            anchor: 0,
            caret: f.value.len(),
        },
        "char-backward" if selected && !extend => moved(start),
        "char-forward" if selected && !extend => moved(end),
        "char-backward" => moved(p.saturating_sub(1)),
        "char-forward" => moved((p + 1).min(n)),
        "word-backward" => moved(word_back(back)),
        "word-forward" => moved(word_forward(forward)),
        "line-start" => moved(line(back).0),
        "line-end" => moved(line(forward).1),
        "input-start" => moved(0),
        "input-end" => moved(n),
        "delete-char-backward" => delete(p.saturating_sub(1), p),
        "delete-char-forward" => delete(p, (p + 1).min(n)),
        "delete-word-backward" => delete(word_back(p), p),
        "delete-word-forward" => delete(p, word_forward(p)),
        "delete-to-line-start" => delete(line(p).0, p),
        "delete-to-line-end" => delete(p, line(p).1),
        _ => return None,
    })
}

/// The index of the character a byte offset is at, or the one it is in.
fn index_of(c: &[&str], byte: usize) -> usize {
    let mut at = 0;
    for (i, g) in c.iter().enumerate() {
        if at + g.len() > byte {
            return i;
        }
        at += g.len();
    }
    c.len()
}

/// The byte offset of character `i`.
fn byte_of(c: &[&str], i: usize) -> usize {
    c[..i].iter().map(|g| g.len()).sum()
}

/// A space: a character whose first code point is Unicode white space. A
/// line break is one.
fn is_space(g: &str) -> bool {
    g.chars().next().is_some_and(char::is_whitespace)
}

fn is_break(g: &str) -> bool {
    matches!(g, "\n" | "\r\n" | "\r")
}

/// A count of characters in `s`, for a caret's place.
pub(crate) fn chars(s: &str) -> usize {
    s.graphemes(true).count()
}

/// The byte offset of character `i` of `s`, or its end.
pub(crate) fn byte_at(s: &str, i: usize) -> usize {
    s.grapheme_indices(true).nth(i).map_or(s.len(), |(b, _)| b)
}

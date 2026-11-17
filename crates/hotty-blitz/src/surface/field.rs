//! `a=field` (SPEC §6.4): the program changes what the user changes in a
//! control, if the user has not changed it since the program last heard.
//! Also the controls' revs, which tell the two apart, and `caret` (§9),
//! which tells the program where the user left a text field's selection.

use super::{FieldState, HISTORY, Surface};
use crate::body::Detail;
use crate::edit;
use crate::input::Event;
use blitz_dom::{BaseDocument, NodeId, local_name};
use hotty::host::{ControlKind, control_kind, field_fits};
use hotty::{FieldChange, Message};
use std::collections::BTreeMap;
use stylo_dom::ElementState;

type Error = (&'static str, String);

/// A text field's selection, its anchor then its caret, in bytes of its
/// value, and its rev: what a `caret` carries (SPEC §9).
pub(super) type Caret = ((i64, i64), i64);

impl Surface {
    /// Whether the document has an element with id `t`.
    pub fn has_element(&self, t: &str) -> bool {
        self.doc.get_element_by_id(t).is_some()
    }

    /// `a=field` on element `t`, with `change` read from `cmd`'s body
    /// (SPEC §6.4). After the shape `hotty::host::field_of` checked: a
    /// target that is not a control, a field not for it and a select value
    /// with no option are `EINVAL`, then a `rev` that is not the control's,
    /// or an input method composing, `ESTALE`, and only then a position
    /// past the end of its value or inside a code point `EINVAL`. Nothing
    /// changes unless all pass, and nothing is sent.
    pub fn field(&mut self, t: &str, change: &FieldChange, cmd: &Message) -> Result<(), Error> {
        // Blitz makes a field's editor, and a checkbox's state, at layout.
        self.resolve();
        let id = self
            .doc
            .get_element_by_id(t)
            .ok_or_else(|| ("ENOTARGET", t.to_string()))?;
        let (tag, ty) = {
            let el = self.doc.get_node(id).and_then(|n| n.element_data());
            let tag = el.map(|e| e.name.local.to_string()).unwrap_or_default();
            let ty = el
                .and_then(|e| e.attr(local_name!("type")))
                .map(str::to_string);
            (tag, ty)
        };
        let kind = control_kind(&tag, ty.as_deref())
            .ok_or_else(|| ("EINVAL", format!("{t} is not a control")))?;
        field_fits(cmd, change, kind).map_err(|e| ("EINVAL", e.detail))?;
        let option = match (kind, &change.value) {
            (ControlKind::Select, Some(v)) => Some(
                self.doc
                    .select_options(id)
                    .into_iter()
                    .find(|&o| self.doc.option_value(o) == *v)
                    .ok_or_else(|| ("EINVAL", format!("{t} has no option {v:?}")))?,
            ),
            _ => None,
        };
        let rev = self.rev(id);
        if change.rev != rev || self.composing(id) {
            return Err(("ESTALE", rev.to_string()));
        }
        let scale = self.viewport.2 as f64;
        self.damage.touch(&self.doc, id, scale);
        match kind {
            ControlKind::Text => {
                let ty = ty.as_deref().unwrap_or("text").to_ascii_lowercase();
                self.field_text(id, &ty, tag == "textarea", change)?;
            }
            ControlKind::Check => {
                let on = change.checked == Some(true);
                if on
                    && ty
                        .as_deref()
                        .is_some_and(|t| t.eq_ignore_ascii_case("radio"))
                {
                    for other in radio_group(&self.doc, id) {
                        if other != id {
                            self.damage.touch(&self.doc, other, scale);
                            self.set_checked(other, false);
                        }
                    }
                }
                self.set_checked(id, on);
            }
            ControlKind::Select => {
                if let Some(o) = option {
                    self.doc.pick_option(id, o);
                }
            }
            ControlKind::Other => {
                if self.date_typed.is_some_and(|(n, ..)| n == id) {
                    self.date_typed = None;
                }
                if let Some(v) = &change.value {
                    let ty = ty.as_deref().unwrap_or("").to_ascii_lowercase();
                    let v = sanitize_other(&ty, &self.doc, id, v);
                    self.doc
                        .mutate()
                        .set_attribute(id, crate::delta::qual("value"), &v);
                }
            }
        }
        self.damage.touch(&self.doc, id, scale);
        self.dirty = true;
        Ok(())
    }

    /// `a=field` in text field `id`, of input type `ty`: the edit, the
    /// selection after it, and one edit of its history (SPEC §6.4, §10.2).
    fn field_text(
        &mut self,
        id: NodeId,
        ty: &str,
        textarea: bool,
        c: &FieldChange,
    ) -> Result<(), Error> {
        let editor = self.field_state(id);
        let before = editor.clone().unwrap_or_else(|| {
            let value = self
                .doc
                .get_node(id)
                .and_then(|n| n.attr(local_name!("value")))
                .unwrap_or_default()
                .to_string();
            let end = value.len();
            FieldState {
                value,
                anchor: end,
                focus: end,
            }
        });
        // An input drops line breaks; a textarea's are LF (SPEC §6.4).
        let lines = |s: &str| {
            if textarea {
                s.replace("\r\n", "\n").replace('\r', "\n")
            } else {
                s.replace(['\r', '\n'], "")
            }
        };
        let old = &before.value;
        let (new, caret) = if let Some(value) = &c.value {
            let v = sanitize_text(ty, &lines(value));
            let end = v.len();
            (v, end)
        } else if let Some(text) = &c.text {
            let (lo, hi) = match (c.from, c.to) {
                (Some(from), Some(to)) => (position(old, from)?, position(old, to)?),
                _ => (
                    before.anchor.min(before.focus),
                    before.anchor.max(before.focus),
                ),
            };
            let text = lines(text);
            (
                format!("{}{text}{}", &old[..lo], &old[hi..]),
                lo + text.len(),
            )
        } else {
            (old.clone(), before.focus)
        };
        let (anchor, focus) = match c.sel {
            Some((a, f)) => (
                edit::cluster_start(&new, position(&new, a)?),
                edit::cluster_start(&new, position(&new, f)?),
            ),
            None => (caret, caret),
        };
        if editor.is_some() {
            self.doc.set_text_input(id, &new, anchor, focus);
        } else if new != *old {
            self.doc
                .mutate()
                .set_attribute(id, crate::delta::qual("value"), &new);
        }
        // One edit of its history, which ends a run of typing; `sel` alone
        // edits nothing, but ends it too.
        let after = FieldState {
            value: new.clone(),
            anchor,
            focus,
        };
        let h = self.histories.entry(id).or_default();
        if h.after.value != before.value {
            *h = Default::default();
        }
        if new != before.value {
            h.undo.push(before.clone());
            if h.undo.len() > HISTORY {
                h.undo.remove(0);
            }
            h.redo.clear();
        }
        h.run = None;
        h.after = after;
        // Focus leaving the field commits the user's edits only (§6.4).
        if let Some((f, v)) = &mut self.focus_value
            && *f == id
            && *v == before.value
        {
            *v = new;
        }
        Ok(())
    }

    /// Checks or unchecks checkbox or radio button `id`.
    fn set_checked(&mut self, id: NodeId, on: bool) {
        let has = self
            .doc
            .get_node(id)
            .and_then(|n| n.element_data())
            .and_then(|e| e.checkbox_input_checked())
            .is_some();
        if has {
            self.doc
                .snapshot_node_and(id, ElementState::CHECKED, |node| {
                    if let Some(el) = node.element_data_mut() {
                        el.set_checkbox_input_checked(on);
                    }
                    node.mark_ancestors_dirty();
                });
        } else if on {
            self.doc
                .mutate()
                .set_attribute(id, crate::delta::qual("checked"), "");
        } else {
            self.doc
                .mutate()
                .clear_attribute(id, crate::delta::qual("checked"));
        }
    }

    /// Whether an input method is composing text in field `id`.
    fn composing(&self, id: NodeId) -> bool {
        self.doc
            .get_node(id)
            .and_then(|n| n.element_data())
            .and_then(|e| e.text_input_data())
            .is_some_and(|t| t.editor.is_composing())
    }

    /// Control `id`'s rev (SPEC §6.4).
    pub(super) fn rev(&self, id: NodeId) -> i64 {
        rev(&self.revs, &self.doc, id)
    }

    /// Where text field `id` has its selection, and its rev, when it has
    /// `caret` in its `data-on` and an id to name it by.
    fn caret_state(&self, id: NodeId) -> Option<Caret> {
        if !self.listens(id, "caret") || self.id_of(id).is_none() || self.date_type(id).is_some() {
            return None;
        }
        let (_, anchor, focus) = self.doc.text_input_selection(id)?;
        Some(((anchor as i64, focus as i64), self.rev(id)))
    }

    /// The text fields an input may move the selection of, with where
    /// each has it now: the focused one, and `also`.
    pub(super) fn watch(&self, also: Option<NodeId>) -> Vec<(NodeId, Caret)> {
        let mut watched: Vec<(NodeId, Caret)> = Vec::new();
        for id in self.focused().into_iter().chain(also) {
            if watched.iter().all(|w| w.0 != id)
                && let Some(c) = self.caret_state(id)
            {
                watched.push((id, c));
            }
        }
        watched
    }

    /// After an input: `caret` for each field `watched` whose selection or
    /// rev it changed, unless an `input` of the field carries the
    /// selection (SPEC §9). One goes before the first other event of its
    /// field; with none, it waits for the next frame when one was sent in
    /// this frame already. A `caret` owed from before goes the same way.
    pub(super) fn carets(&mut self, mut watched: Vec<(NodeId, Caret)>, events: &mut Vec<Event>) {
        if let Some((id, was)) = self.caret_owed.take() {
            match watched.iter_mut().find(|w| w.0 == id) {
                Some(w) => w.1 = was,
                None => watched.push((id, was)),
            }
        }
        for (id, was) in watched {
            let (Some(now), Some(target)) = (self.caret_state(id), self.id_of(id)) else {
                continue;
            };
            let said = events.iter().any(|e| {
                e.kind == "input"
                    && e.target == target
                    && matches!(e.detail, Some(Detail::Value { sel: Some(_), .. }))
            });
            if said || now == was {
                continue;
            }
            let first = events.iter().position(|e| e.target == target);
            if first.is_none() && self.caret_sent {
                self.caret_owed = Some((id, was));
                // The next frame sends it.
                self.dirty = true;
                continue;
            }
            self.caret_sent = true;
            events.insert(first.unwrap_or(events.len()), caret_event(target, now));
        }
    }

    /// A new frame (Host::render_dirty): the `caret` held back in the last
    /// one, if the field's selection is still not where the program last
    /// heard it.
    pub fn new_frame(&mut self) -> Option<Event> {
        self.caret_sent = false;
        let (id, was) = self.caret_owed.take()?;
        if self.is_detached() {
            return None;
        }
        let now = self.caret_state(id).filter(|&c| c != was)?;
        self.caret_sent = true;
        Some(caret_event(self.id_of(id)?, now))
    }
}

fn caret_event(target: String, (sel, rev): Caret) -> Event {
    Event {
        kind: "caret",
        target,
        detail: Some(Detail::Caret { sel, rev }),
    }
}

/// Position `p` in `value`: `EINVAL` when it is negative, past the end,
/// or inside a code point (SPEC §6.4). Checked only once the command is
/// known not to be stale.
fn position(value: &str, p: i64) -> Result<usize, Error> {
    usize::try_from(p)
        .ok()
        .filter(|&b| value.is_char_boundary(b))
        .ok_or_else(|| {
            let why = if p < 0 {
                "negative, not a place in"
            } else if p as u64 > value.len() as u64 {
                "past the end of"
            } else {
                "inside a code point of"
            };
            ("EINVAL", format!("position {p} is {why} the value"))
        })
}

/// Control `id`'s rev: a radio button's is its group's highest.
pub(super) fn rev(revs: &BTreeMap<NodeId, i64>, doc: &BaseDocument, id: NodeId) -> i64 {
    radio_group(doc, id)
        .iter()
        .filter_map(|n| revs.get(n))
        .copied()
        .max()
        .unwrap_or(0)
}

/// The user changed control `id`: its rev, and a radio button's whole
/// group's, becomes one more than its rev was. Returns the new rev.
pub(super) fn bump(revs: &mut BTreeMap<NodeId, i64>, doc: &BaseDocument, id: NodeId) -> i64 {
    let group = radio_group(doc, id);
    let r = group
        .iter()
        .filter_map(|n| revs.get(n))
        .copied()
        .max()
        .unwrap_or(0)
        + 1;
    for n in group {
        revs.insert(n, r);
    }
    r
}

/// The radio buttons of `id`'s group, HTML's: one `name`, in one form.
/// Just `id` for any other element, and a radio button with no name.
pub(super) fn radio_group(doc: &BaseDocument, id: NodeId) -> Vec<NodeId> {
    let name = |n: NodeId| {
        let el = doc.get_node(n)?.element_data()?;
        let radio = &*el.name.local == "input"
            && el
                .attr(local_name!("type"))
                .is_some_and(|t| t.eq_ignore_ascii_case("radio"));
        radio
            .then(|| el.attr(local_name!("name")).unwrap_or_default().to_string())
            .filter(|n| !n.is_empty())
    };
    let Some(group) = name(id) else {
        return vec![id];
    };
    let form = form_of(doc, id);
    let mut ids = Vec::new();
    let mut stack = vec![doc.root_node().id];
    while let Some(n) = stack.pop() {
        let Some(node) = doc.get_node(n) else {
            continue;
        };
        if name(n).as_deref() == Some(group.as_str()) && form_of(doc, n) == form {
            ids.push(n);
        }
        stack.extend(node.children.iter().rev().copied());
    }
    ids
}

/// `id`'s form owner, HTML's, as Blitz finds it: the form its `form`
/// attribute names, or else the form it is in, if any.
fn form_of(doc: &BaseDocument, id: NodeId) -> Option<NodeId> {
    let is_form = |n: NodeId| {
        doc.get_node(n)
            .and_then(|n| n.element_data())
            .is_some_and(|e| &*e.name.local == "form")
    };
    let named = doc
        .get_node(id)?
        .attr(local_name!("form"))
        .and_then(|f| doc.get_element_by_id(f))
        .filter(|&f| is_form(f));
    if named.is_some() {
        return named;
    }
    let mut n = doc.get_node(id)?.parent;
    while let Some(p) = n {
        if is_form(p) {
            return Some(p);
        }
        n = doc.get_node(p)?.parent;
    }
    None
}

/// A text field's value as a script sets it, line breaks already gone
/// from an input (HTML's value sanitization): an email or a url loses the
/// white space at its ends, and a number that does not parse is empty.
fn sanitize_text(ty: &str, v: &str) -> String {
    match ty {
        "email" | "url" => v
            .trim_matches(|c: char| c.is_ascii_whitespace())
            .to_string(),
        "number" if !valid_float(v) => String::new(),
        _ => v.to_string(),
    }
}

/// Any other input's value as a script sets it (HTML's value
/// sanitization): a range clamps and steps it, a colour is a lowercase
/// `#rrggbb` or black, and a date or a time that does not parse is empty.
fn sanitize_other(ty: &str, doc: &BaseDocument, id: NodeId, v: &str) -> String {
    let v: String = v.chars().filter(|&c| c != '\r' && c != '\n').collect();
    let attr = |name| {
        doc.get_node(id)
            .and_then(|n| n.attrs())
            .and_then(|a| a.iter().find(|a| &*a.name.local == name))
            .map(|a| a.value.to_string())
    };
    let num = |name, default: f64| {
        attr(name)
            .filter(|s| valid_float(s))
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(default)
    };
    match ty {
        "range" => {
            let min = num("min", 0.0);
            let max = num("max", 100.0).max(min);
            let fallback = min + (max - min) / 2.0;
            let mut x = Some(v.as_str())
                .filter(|s| valid_float(s))
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(fallback)
                .clamp(min, max);
            let any = attr("step").is_some_and(|s| s.eq_ignore_ascii_case("any"));
            let step = Some(num("step", 1.0)).filter(|s| *s > 0.0).unwrap_or(1.0);
            if !any {
                x = min + ((x - min) / step).round() * step;
                if x > max {
                    x -= step;
                }
            }
            format!("{x}")
        }
        "color" => {
            let hex =
                v.len() == 7 && v.starts_with('#') && v[1..].bytes().all(|b| b.is_ascii_hexdigit());
            if hex {
                v.to_ascii_lowercase()
            } else {
                "#000000".to_string()
            }
        }
        _ => blitz_dom::date_time::sanitize(ty, &v),
    }
}

/// HTML's valid floating-point number: `-`, digits with a fraction or
/// either alone, and an exponent.
fn valid_float(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    let (mantissa, exp) = match s.find(['e', 'E']) {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    let digits = |d: &str| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit());
    let mantissa_ok = match mantissa.split_once('.') {
        Some((int, frac)) => (int.is_empty() || digits(int)) && digits(frac),
        None => digits(mantissa),
    };
    let exp_ok = exp.is_none_or(|e| digits(e.strip_prefix(['+', '-']).unwrap_or(e)));
    mantissa_ok && exp_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats() {
        for ok in ["0", "-1", "1.5", ".5", "-.5", "1e3", "1E-3", "2.5e+2"] {
            assert!(valid_float(ok), "{ok}");
        }
        for bad in ["", "+1", "1.", "-", "e3", "1e", " 1", "1 ", "0x1", "1.2.3"] {
            assert!(!valid_float(bad), "{bad}");
        }
    }

    #[test]
    fn positions() {
        assert_eq!(position("héllo", 3), Ok(3));
        assert!(position("héllo", 2).is_err());
        assert!(position("ab", 3).is_err());
        assert_eq!(position("ab", 2), Ok(2));
        assert!(position("ab", -1).is_err());
    }
}

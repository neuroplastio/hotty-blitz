//! Deltas (SPEC §6): operations on one surface's document,
//! addressed by element id, and the morph that keeps element identity.
//!
//! Fragments are parsed into a separate **scratch** document, never into the
//! live one: html5ever parses a fragment under the document node, and in the
//! live document that would register ids, form owners and image loads for
//! nodes the morph is about to throw away. Only what the morph actually
//! inserts is built in the live document.

use blitz_dom::{
    Attribute, BaseDocument, DocumentMutator, LocalName, NodeData, NodeId, QualName, local_name, ns,
};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaError {
    pub code: &'static str,
    pub detail: String,
}

impl DeltaError {
    fn new(code: &'static str, detail: impl Into<String>) -> DeltaError {
        DeltaError {
            code,
            detail: detail.into(),
        }
    }
}

/// Applies one delta. `touch` is called with the document as it was before
/// the change, for every node whose painted area the delta may change, so
/// the caller can repaint only those (see paint.rs).
#[allow(clippy::too_many_arguments)]
pub fn apply(
    doc: &mut BaseDocument,
    scratch: &mut BaseDocument,
    op: &str,
    target: Option<&str>,
    key: Option<&str>,
    payload: &str,
    touch: &mut dyn FnMut(&BaseDocument, NodeId),
) -> Result<(), DeltaError> {
    let parent = |doc: &BaseDocument, id: NodeId| doc.get_node(id).and_then(|n| n.parent);
    let find = |doc: &BaseDocument, id: Option<&str>| -> Result<NodeId, DeltaError> {
        let id =
            id.ok_or_else(|| DeltaError::new("EINVAL", format!("op={op} needs t=<element id>")))?;
        doc.get_element_by_id(id)
            .ok_or_else(|| DeltaError::new("ENOTARGET", id.to_string()))
    };
    let attr_name = || -> Result<QualName, DeltaError> {
        let k = key.ok_or_else(|| DeltaError::new("EINVAL", format!("op={op} needs k=<name>")))?;
        Ok(qual(k))
    };
    // The element whose children the fragment becomes decides how it parses
    // (a `<tr>` only parses inside a table).
    let context = |doc: &BaseDocument, t: NodeId, children_of_target: bool| -> String {
        let of = if children_of_target {
            Some(t)
        } else {
            doc.get_node(t).and_then(|n| n.parent)
        };
        of.and_then(|id| doc.get_node(id))
            .and_then(|n| n.element_data())
            .map(|e| e.name.local.to_string())
            .unwrap_or_else(|| "body".into())
    };
    match op {
        "morph" | "" => match target {
            Some(_) => {
                let t = find(doc, target)?;
                let frag = parse(scratch, payload, &context(doc, t, false));
                let elements: Vec<NodeId> = frag
                    .iter()
                    .copied()
                    .filter(|&k| is_element(scratch, k))
                    .collect();
                if elements.len() != 1 {
                    touch(doc, t);
                    if let Some(p) = parent(doc, t) {
                        touch(doc, p);
                    }
                }
                let mut m = doc.mutate();
                if elements.len() == 1 {
                    // The morph touches exactly what it changes.
                    morph_node(&mut m, t, scratch, elements[0], touch);
                } else {
                    let built: Vec<NodeId> =
                        frag.iter().map(|&k| build(&mut m, scratch, k)).collect();
                    m.insert_nodes_before(t, &built);
                    m.remove_and_drop_node(t);
                }
                Ok(())
            }
            None => {
                // No target: match the payload's top-level elements by id.
                let frag = parse(scratch, payload, "body");
                let mut missing = Vec::new();
                let mut m = doc.mutate();
                for k in frag.iter().copied().filter(|&k| is_element(scratch, k)) {
                    let Some(id) = attr(scratch, k, "id") else {
                        missing.push("(element without id)".to_string());
                        continue;
                    };
                    match m.doc.get_element_by_id(&id) {
                        Some(old) => {
                            morph_node(&mut m, old, scratch, k, touch);
                        }
                        None => missing.push(id),
                    }
                }
                drop(m);
                if missing.is_empty() {
                    Ok(())
                } else {
                    Err(DeltaError::new("ENOTARGET", missing.join(",")))
                }
            }
        },
        "inner" => {
            let t = find(doc, target)?;
            let frag = parse(scratch, payload, &context(doc, t, true));
            let mut m = doc.mutate();
            morph_children(&mut m, t, scratch, &frag, touch);
            Ok(())
        }
        "replace" => {
            let t = find(doc, target)?;
            touch(doc, t);
            if let Some(p) = parent(doc, t) {
                touch(doc, p);
            }
            let frag = parse(scratch, payload, &context(doc, t, false));
            let mut m = doc.mutate();
            let built: Vec<NodeId> = frag.iter().map(|&k| build(&mut m, scratch, k)).collect();
            m.insert_nodes_before(t, &built);
            m.remove_and_drop_node(t);
            Ok(())
        }
        "append" | "prepend" => {
            let t = find(doc, target)?;
            touch(doc, t);
            let frag = parse(scratch, payload, &context(doc, t, true));
            let mut m = doc.mutate();
            // As Turbo: a child with the same id is morphed in place, not duplicated.
            let mut fresh = Vec::new();
            for k in frag {
                let existing = attr(scratch, k, "id")
                    .and_then(|id| m.doc.get_element_by_id(&id))
                    .filter(|&old| m.parent_id(old) == Some(t));
                match existing {
                    Some(old) => {
                        morph_node(&mut m, old, scratch, k, touch);
                    }
                    None => fresh.push(build(&mut m, scratch, k)),
                }
            }
            if op == "append" {
                m.append_children(t, &fresh);
            } else {
                m.prepend_nodes(t, &fresh);
            }
            Ok(())
        }
        "before" | "after" => {
            let t = find(doc, target)?;
            if let Some(p) = parent(doc, t) {
                touch(doc, p);
            }
            let frag = parse(scratch, payload, &context(doc, t, false));
            let mut m = doc.mutate();
            let built: Vec<NodeId> = frag.iter().map(|&k| build(&mut m, scratch, k)).collect();
            if op == "before" {
                m.insert_nodes_before(t, &built);
            } else {
                m.insert_nodes_after(t, &built);
            }
            Ok(())
        }
        "remove" => {
            let t = find(doc, target)?;
            touch(doc, t);
            if let Some(p) = parent(doc, t) {
                touch(doc, p);
            }
            doc.mutate().remove_and_drop_node(t);
            Ok(())
        }
        "attr" => {
            let t = find(doc, target)?;
            let name = attr_name()?;
            if keeps_state(doc, t, &name) {
                return Ok(());
            }
            touch(doc, t);
            doc.mutate().set_attribute(t, name, payload);
            Ok(())
        }
        "unattr" => {
            let t = find(doc, target)?;
            let name = attr_name()?;
            if keeps_state(doc, t, &name) {
                return Ok(());
            }
            touch(doc, t);
            doc.mutate().clear_attribute(t, name);
            Ok(())
        }
        "text" => {
            let t = find(doc, target)?;
            touch(doc, t);
            let mut m = doc.mutate();
            let kids = m.child_ids(t);
            if kids.len() == 1 && m.doc.get_node(kids[0]).is_some_and(|n| n.is_text_node()) {
                m.set_node_text(kids[0], payload);
            } else {
                m.remove_and_drop_all_children(t);
                let text = m.create_text_node(payload);
                m.append_children(t, &[text]);
            }
            Ok(())
        }
        "var" => {
            let t = find(doc, target)?;
            touch(doc, t);
            let k = key.ok_or_else(|| DeltaError::new("EINVAL", "op=var needs k=<name>"))?;
            let name = if k.starts_with("--") {
                k.to_string()
            } else {
                format!("--{k}")
            };
            doc.mutate().set_style_property(t, &name, payload);
            Ok(())
        }
        other => Err(DeltaError::new("EINVAL", format!("unknown op={other}"))),
    }
}

pub fn qual(name: &str) -> QualName {
    QualName::new(None, ns!(), LocalName::from(name))
}

/// Parses `html` into the scratch document as the children of an element
/// named `context`; returns those children.
fn parse(scratch: &mut BaseDocument, html: &str, context: &str) -> Vec<NodeId> {
    let (wrap_open, wrap_close, depth) = match context {
        "table" | "tbody" | "thead" | "tfoot" => ("<table><tbody>", "</tbody></table>", 2),
        "tr" => ("<table><tbody><tr>", "</tr></tbody></table>", 3),
        "ul" | "ol" | "select" | "datalist" => ("", "", 0),
        _ => ("", "", 0),
    };
    let body = scratch
        .find_body_node()
        .map(|n| n.id)
        .unwrap_or_else(|| scratch.root_element().id);
    {
        let mut m = scratch.mutate();
        if depth == 0 {
            m.set_inner_html(body, html);
        } else {
            m.set_inner_html(body, &format!("{wrap_open}{html}{wrap_close}"));
        }
    }
    let mut parent = body;
    for _ in 0..depth {
        match scratch.get_node(parent).and_then(|n| {
            n.children
                .iter()
                .copied()
                .find(|&c| scratch.get_node(c).is_some_and(|n| n.is_element()))
        }) {
            Some(c) => parent = c,
            None => break,
        }
    }
    scratch
        .get_node(parent)
        .map(|n| n.children.to_vec())
        .unwrap_or_default()
}

/// Builds a detached copy of the scratch subtree at `s` in the live document.
fn build(m: &mut DocumentMutator<'_>, scratch: &BaseDocument, s: NodeId) -> NodeId {
    let Some(node) = scratch.get_node(s) else {
        return m.create_text_node("");
    };
    match &node.data {
        NodeData::Element(el) => {
            let attrs: Vec<Attribute> = el.attrs().to_vec();
            let id = m.create_element(el.name.clone(), attrs);
            let kids: Vec<NodeId> = node
                .children
                .iter()
                .map(|&c| build(m, scratch, c))
                .collect();
            if !kids.is_empty() {
                m.append_children(id, &kids);
            }
            id
        }
        NodeData::Text(t) => m.create_text_node(&t.content),
        NodeData::Comment { contents } => m.create_comment_node(contents),
        _ => m.create_text_node(""),
    }
}

fn is_element(doc: &BaseDocument, id: NodeId) -> bool {
    doc.get_node(id).is_some_and(|n| n.is_element())
}

fn attr(doc: &BaseDocument, id: NodeId, name: &str) -> Option<String> {
    doc.get_node(id)?
        .attrs()?
        .iter()
        .find(|a| &*a.name.local == name)
        .map(|a| a.value.clone())
}

#[derive(PartialEq, Eq)]
enum Shape {
    Element(QualName, Option<String>),
    Text(String),
    Comment,
    Other,
}

fn shape(doc: &BaseDocument, id: NodeId) -> Shape {
    let Some(node) = doc.get_node(id) else {
        return Shape::Other;
    };
    match &node.data {
        NodeData::Element(el) => Shape::Element(
            el.name.clone(),
            el.attr(local_name!("id")).map(str::to_string),
        ),
        NodeData::Text(t) => Shape::Text(t.content.clone()),
        NodeData::Comment { .. } => Shape::Comment,
        _ => Shape::Other,
    }
}

/// Makes `old` (live) look like `new` (scratch), keeping `old`'s identity
/// wherever the two are the same kind of node. Returns the node in its
/// place: `old`, or what replaced it (`old` is then freed).
pub fn morph_node(
    m: &mut DocumentMutator<'_>,
    old: NodeId,
    scratch: &BaseDocument,
    new: NodeId,
    touch: &mut dyn FnMut(&BaseDocument, NodeId),
) -> NodeId {
    match (shape(m.doc, old), shape(scratch, new)) {
        (Shape::Text(a), Shape::Text(b)) => {
            if a != b {
                touch(m.doc, old);
                m.set_node_text(old, &b);
            }
            old
        }
        (Shape::Comment, Shape::Comment) => old,
        (Shape::Element(na, ia), Shape::Element(nb, ib))
            if na == nb && (ia == ib || ia.is_none() || ib.is_none()) =>
        {
            sync_attributes(m, old, scratch, new, touch);
            let kids = scratch
                .get_node(new)
                .map(|n| n.children.to_vec())
                .unwrap_or_default();
            morph_children(m, old, scratch, &kids, touch);
            old
        }
        _ => {
            touch(m.doc, old);
            if let Some(p) = m.doc.get_node(old).and_then(|n| n.parent) {
                touch(m.doc, p);
            }
            let built = build(m, scratch, new);
            m.insert_nodes_before(old, &[built]);
            m.remove_and_drop_node(old);
            built
        }
    }
}

/// SPEC §6.2: a program's `value` or `checked` sets a control's current
/// state, except while the control is focused: the user is editing it, and
/// keeps what they typed. In Blitz the attribute is the current state, so a
/// focused control keeps the attribute too, whichever op changes it.
fn keeps_state(doc: &BaseDocument, id: NodeId, name: &QualName) -> bool {
    matches!(&*name.local, "value" | "checked") && crate::surface::focused_node(doc) == Some(id)
}

fn sync_attributes(
    m: &mut DocumentMutator<'_>,
    old: NodeId,
    scratch: &BaseDocument,
    new: NodeId,
    touch: &mut dyn FnMut(&BaseDocument, NodeId),
) {
    let collect = |doc: &BaseDocument, id: NodeId| -> Vec<(QualName, String)> {
        doc.get_node(id)
            .and_then(|n| n.attrs())
            .map(|a| {
                a.iter()
                    .map(|a| (a.name.clone(), a.value.clone()))
                    .collect()
            })
            .unwrap_or_default()
    };
    let old_attrs = collect(m.doc, old);
    let new_attrs = collect(scratch, new);
    for (name, value) in &new_attrs {
        if keeps_state(m.doc, old, name) {
            continue;
        }
        if !old_attrs.iter().any(|(n, v)| n == name && v == value) {
            touch(m.doc, old);
            m.set_attribute(old, name.clone(), value);
        }
    }
    for (name, _) in &old_attrs {
        if keeps_state(m.doc, old, name) {
            continue;
        }
        if !new_attrs.iter().any(|(n, _)| n == name) {
            touch(m.doc, old);
            m.clear_attribute(old, name.clone());
        }
    }
}

/// Morphs the children of `old_parent` (live) into `new_kids` (scratch).
pub fn morph_children(
    m: &mut DocumentMutator<'_>,
    old_parent: NodeId,
    scratch: &BaseDocument,
    new_kids: &[NodeId],
    touch: &mut dyn FnMut(&BaseDocument, NodeId),
) {
    let old_kids = m.child_ids(old_parent).to_vec();
    let new_ids: HashSet<String> = new_kids
        .iter()
        .filter_map(|&k| attr(scratch, k, "id"))
        .collect();

    let mut used: HashSet<NodeId> = HashSet::new();
    // Each entry: an old node to keep (morphed) or a new one to build.
    let mut plan: Vec<(Option<NodeId>, NodeId)> = Vec::with_capacity(new_kids.len());
    let mut cursor = 0usize;
    for &nk in new_kids {
        let nk_shape = shape(scratch, nk);
        let matched = match &nk_shape {
            Shape::Element(_, Some(id)) => old_kids.iter().copied().find(|&ok| {
                !used.contains(&ok) && attr(m.doc, ok, "id").as_deref() == Some(id.as_str())
            }),
            _ => {
                // Positional match on the same kind of node, skipping old
                // children whose id the new list still wants.
                let mut found = None;
                for (i, &ok) in old_kids.iter().enumerate().skip(cursor) {
                    if used.contains(&ok) {
                        continue;
                    }
                    let same = match (&shape(m.doc, ok), &nk_shape) {
                        (Shape::Element(_, Some(id)), _) if new_ids.contains(id) => false,
                        (Shape::Element(a, None), Shape::Element(b, None)) => a == b,
                        (Shape::Text(_), Shape::Text(_)) => true,
                        (Shape::Comment, Shape::Comment) => true,
                        _ => false,
                    };
                    if same {
                        found = Some(ok);
                        cursor = i + 1;
                        break;
                    }
                }
                found
            }
        };
        if let Some(ok) = matched {
            used.insert(ok);
        }
        plan.push((matched, nk));
    }
    let structural =
        plan.iter().any(|(keep, _)| keep.is_none()) || old_kids.iter().any(|ok| !used.contains(ok));
    if structural {
        // Children come or go: the parent's box can reflow.
        touch(m.doc, old_parent);
    }
    for &ok in &old_kids {
        if !used.contains(&ok) {
            m.remove_and_drop_node(ok);
        }
    }
    // Morph the kept nodes, build the new ones, and put everything in order
    // under `old_parent`, moving (not recreating) the kept ones.
    for (i, (keep, nk)) in plan.into_iter().enumerate() {
        // A kept node of another kind (an id moved to another tag) is
        // replaced, and its replacement is what goes in order.
        let want = match keep {
            Some(ok) => morph_node(m, ok, scratch, nk, touch),
            None => build(m, scratch, nk),
        };
        let current = m.child_ids(old_parent);
        match current.get(i) {
            Some(&have) if have == want => {}
            Some(&have) => m.insert_nodes_before(have, &[want]),
            None => m.append_children(old_parent, &[want]),
        }
    }
}

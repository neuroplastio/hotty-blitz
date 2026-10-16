---
title: "A control in the disabled state acts disabled"
patch: 0003-blitz-dom-blitz-paint-a-control-in-the-disabled-stat.patch
upstream-fit: yes
---

**What it changes:** pointer handling and form-control painting asked for
the `disabled` attribute. Blitz already keeps a control's disabled state in
its `ElementState`, which `:disabled` matches; they now ask
`ElementData::is_disabled`, the attribute or the state. Nothing changes for
a document whose state comes from its attributes. An embedder that
disables a control with `Node::disable`, leaving the attributes alone, gets
what the attribute gives: no focus, no toggle, no submit, no selection, and
a grey checkbox.

**Why we carry it:** a detached HOTTY surface's controls act disabled
(SPEC §5.5) while the document keeps the program's attributes.

**For upstream:** yes. It is small, and makes the state and the attribute
agree everywhere.

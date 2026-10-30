---
title: "An item resized by its own style lays its container out again"
patch: 0047-blitz-dom-an-item-resized-by-its-own-style-lays-its-.patch
upstream-fit: maybe
---

**What it changes:** the speculative relayout of "A delta costs the change,
not the document" kept a flex or grid container's old layout when each
changed item, asked again the questions the container asked it last time,
gave the same answers. But the container reads an item's style itself
(width, flex-basis, margins), and asks with what it made of it: an item
whose width changed answered the old questions as before, and kept its old
size. Stylo asks `compute_layout_damage` only for a style change that needs
relayout; it now adds `STYLE_RELAYOUT`, a damage bit of Blitz's own, which
`propagate_damage_flags` records and drops, and `speculate_layout` settles
no container from an item that carries it. A change to an item's content (a
text node's, which damages its parent without Stylo) and a change that only
repaints still speculate.

**Why we carry it:** a program that sets a flex item's width through a
custom property (`width: calc(var(--w) * …)`, changed by a `var` op) saw the
item keep its old width after a window resize. A plain `width` set on a flex
item, or a grid item's, did the same.

**Platform:** neutral. It is Blitz code only, the same on macOS.

**For upstream:** maybe, as part of "A delta costs the change, not the
document": it fixes that patch, and goes wherever it goes.

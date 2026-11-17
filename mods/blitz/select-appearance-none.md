---
title: "A select with appearance none has no native arrow"
patch: 0102-blitz-dom-hide-the-native-select-arrow-with-appearan.patch
upstream-fit: maybe
---

A closed select's arrow is currently a user-agent `::after`. When the
select has `appearance: none`, do not construct that pseudo-element if its
winning `content` comes from the UA sheet; content supplied by an author or
user sheet is retained. Changing `appearance` rebuilds the box, so toggling
it removes or restores the arrow on the next render.

We carry this for custom selects such as Basecoat's: the page supplies its
own background chevron, and the UA triangle previously appeared on top.
Upstream needs the same appearance behavior, though the implementation is
specific to the UA pseudo-element used for Blitz's select widget.

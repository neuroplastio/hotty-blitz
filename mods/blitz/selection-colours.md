---
title: "Selected text takes the colours of its ::selection"
patch: 0018-blitz-dom-blitz-paint-selected-text-takes-the-colour.patch
upstream-fit: yes
---

**What it changes:** a selection was a fixed light blue under text in its
own colours. The highlight is now the inline root's, or the text field's,
`::selection` background, or that blue when it sets none, and the selected
text is drawn again over it, clipped to it, in each run's `::selection`
colour.

**Why we carry it:** hotty's host stylesheet colours selections with the
terminal's palette (SPEC §8).

**For upstream:** yes. It is `::selection` as CSS has it.

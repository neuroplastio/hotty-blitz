---
title: ":focus-visible matches, and a click shows the ring only on a text field"
patch: 0016-blitz-dom-focus-visible-matches-the-focus-ring-a-cli.patch
upstream-fit: yes
---

**What it changes:** `:focus-visible` matched nothing. It now matches the
focus ring (`ElementState::FOCUSRING`), which focus from the keyboard or a
script sets. Focus moved by the pointer (`set_focus_by_pointer`, which
Blitz's own click handling now calls) keeps the ring only on a text field,
as browsers do.

**Why we carry it:** hotty's host stylesheet draws focus rings with
`:focus-visible` (SPEC §8).

**For upstream:** yes. It is the selector as browsers match it.

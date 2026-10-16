---
title: "A select has a selected option, submits it, and draws closed"
patch: 0023-blitz-dom-a-select-has-a-selected-option-submits-it-.patch
upstream-fit: yes
---

**What it changes:** every `<option>` was hidden and nothing kept
selectedness, so a select drew as an empty box and never reached form
data. An option's selectedness is now its `ElementState::CHECKED` bit, as
in Gecko, so `option:checked` matches it, and the mutator settles it with
HTML's selectedness algorithm as options come, go, or have `selected` set
or removed. Form data has the selected options. A select without
`multiple` draws closed, as a browser draws it: the selected option and an
arrow, as wide as its widest option. `BaseDocument::pick_option` picks.
Its list isn't drawn, nor is a `multiple` select or one with `size` above
1.

**Why we carry it:** HOTTY lets a host show no list, and hotty-blitz
picks with the keys (SPEC §10.2).

**For upstream:** yes. Selectedness and the closed select are HTML's, and
a list could come later.

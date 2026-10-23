---
title: "A number input is as wide as a text input"
patch: 0038-blitz-dom-size-a-number-input-like-a-text-input.patch
upstream-fit: yes
---

**What it changes:** `<input type=number>` was missing from the branch
that sizes text inputs, so it fell through to the generic leaf layout,
which sizes a control by its content: an empty number field was only as
wide as its padding and border. It now takes a text input's width,
`size` characters plus the widest glyph less one character, and its
baseline. Chromium narrows a number field to the digits of `min` and
`max` when both are set; this does not.

**Why we carry it:** a settings page's number fields were slivers in
hottyterm.

**For upstream:** yes. Browsers size it this way.

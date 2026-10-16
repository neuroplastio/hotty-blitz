---
title: "A row move can extend the selection"
patch: 0030-blitz-dom-a-row-move-can-extend-the-selection.patch
upstream-fit: yes
---

**What it changes:** `TextInputData::move_rows`, and
`move_text_input_rows`, take `extend`: with it, the selection's anchor
stays and the caret moves, as Shift+ArrowDown does in a browser's
textarea. Without it, a selection collapses first, as before. Parley's
`move_lines` already extends; this passes the flag through.

**Why we carry it:** HOTTY text fields select by rows with the keymap.

**For upstream:** yes. It is small and generic.

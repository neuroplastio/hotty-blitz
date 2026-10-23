---
title: ":hover and :active match the DOM ancestors, a table row among them"
patch: 0037-blitz-dom-propagate-hover-active-along-DOM-ancestors.patch
upstream-fit: yes
---

**What it changes:** Blitz set and cleared the hover and active states
along the target's layout ancestors. The layout tree skips some of the
elements those selectors match: a table's cells are the table's layout
children, so `tr:hover` and `tbody:hover` never matched, and neither did
`:hover` on an element with `display: contents` or on the inline
ancestors of an inline box. The states now follow the target's ancestor
elements in the DOM, which is where `:hover` and `:active` match. Hit
testing still follows layout. The patch is upstream's own fix, #985
(8ea8bd6b), backported to our base with its tests, plus one that hovers
and presses a table cell and checks its row and row group.

**Why we carry it:** a hotty program's table highlights the row under
the pointer with `tr:hover`, as on the web, and in hottyterm the row
never lit up.

**For upstream:** yes. Upstream made the same change in #985, after our
base, so the patch goes once `base` moves past 8ea8bd6b. The table-row
test is the one part upstream doesn't have.

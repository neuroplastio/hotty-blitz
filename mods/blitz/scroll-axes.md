---
title: "A document can lock scrolling along an axis"
patch: 0013-blitz-dom-blitz-paint-a-document-can-lock-scrolling-.patch
upstream-fit: maybe
---

**What it changes:** `BaseDocument::set_scroll_axes` locks an axis: every
scroll target's range along it is empty, whoever scrolls (the user, a
fragment link, a fling, a thumb), locking puts the offsets back to zero,
and no scrollbar is drawn or hit there. A text input still scrolls its own
text to follow the caret.

**Why we carry it:** hotty scrolls a document only along the axes it asks
for (SPEC §5.1, §5.3).

**For upstream:** maybe. It is an embedder API shaped by hotty's model;
another embedder might want it, but nothing in the web platform asks for
it.

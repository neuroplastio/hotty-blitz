---
title: "An embedder can hide the caret, to blink it"
patch: 0020-blitz-dom-blitz-paint-an-embedder-can-hide-the-caret.patch
upstream-fit: maybe
---

**What it changes:** a focused text input's caret was painted solid.
`BaseDocument::set_caret_shown` turns it off and on, and the painter
honours it.

**Why we carry it:** hotty-blitz blinks the caret on the frame clock that
plays animated images, repainting only the input, as a browser blinks it.

**For upstream:** maybe. Blinking is wanted, but upstream may prefer to
blink the caret itself rather than leave it to the embedder.

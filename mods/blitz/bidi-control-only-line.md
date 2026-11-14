---
title: Give bidi-control-only inlines no line box
patch: 0075-Give-bidi-control-only-inlines-no-line-box.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0059-Give-an-inline-of-only-bidi-controls-no-line-box.patch
---

Bidi formatting controls do not by themselves give an inline context a strut-height line. The host test covers control-only text, an out-of-flow child, real text and an in-flow inline box. The current base already avoids a line for an empty `dir` span; the port also handles explicit formatting controls.

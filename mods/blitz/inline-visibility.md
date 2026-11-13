---
title: Honour visibility on inline text
patch: 0061-Honour-visibility-hidden-on-inline-text.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-paint-0.3.0-beta.4/0020-Honour-visibility-hidden-on-inline-text.patch
---

Skip hidden inline glyphs, decorations and run backgrounds while retaining
their layout, and allow visible descendants of a hidden span to paint.
Inline text paints through its root and otherwise misses the normal box
visibility check. This CSS behavior belongs upstream. Adapted from
kitesurf (Apache-2.0) for our text painter.

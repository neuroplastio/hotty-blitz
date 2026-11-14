---
title: Draw visible repeating-gradient tiles
patch: 0073-Draw-only-gradient-tiles-reaching-the-viewport.patch
upstream-fit: maybe
source: kitesurf scripts/patches/blitz-paint-0.3.0-beta.4/0027-Draw-only-the-gradient-tiles-that-reach-the-viewport.patch
---

Cull gradient tiles to the viewport before enforcing a fixed 24,000-tile cap. A tall repeating dot pattern no longer disappears at the old 500-tile limit; a pixel regression checks the initial view and newly visible tiles after scrolling. The cap still bounds work for tiny tiles.

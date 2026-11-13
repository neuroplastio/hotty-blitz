---
title: Expand tabs to CSS tab stops
patch: 0006-Expand-tabs-to-CSS-tab-stops-in-Parley.patch
upstream-fit: yes
source: kitesurf scripts/patches/parley-0.12.0/0006-Expand-horizontal-tabs-to-CSS-tab-size-stops.patch
---

Expand preserved tabs to the configured number or length of space stops
when measuring and laying out text. This fixes preformatted text and is
general text-layout behavior that Parley could support upstream. Adapted
from kitesurf (Apache-2.0) to our inline-edge changes.

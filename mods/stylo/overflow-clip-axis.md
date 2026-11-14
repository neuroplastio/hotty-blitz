---
title: Clip remains non-scrollable beside a scrollable axis
patch: 0006-Stop-escalating-a-clip-overflow-axis-to-hidden.patch
upstream-fit: yes
source: kitesurf scripts/patches/stylo-0.22.0/0011-Stop-escalating-a-clip-overflow-axis-to-hidden.patch
---

When the other overflow axis is scrollable, Stylo now changes `visible`
to `auto` but keeps `clip` non-scrollable. Hotty needs single-axis scroll
containers to preserve their specified clipping and sticky behavior. This
matches the current CSS Overflow definition and should be useful to other
Stylo embedders. The source patch is Apache-2.0.

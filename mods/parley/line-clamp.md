---
title: Add layout line truncation for line clamp
patch: 0007-Add-layout-line-truncation-for-line-clamp.patch
upstream-fit: yes
source: kitesurf scripts/patches/parley-0.12.0/0004-Add-Layout-truncate_lines-for-CSS-line-clamp.patch
---

Expose line truncation after line breaking and recompute height from the last retained line. Blitz uses this for legacy WebKit line clamping; its host test covers changing and removing a clamp across resolves.

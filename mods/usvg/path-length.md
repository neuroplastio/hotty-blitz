---
title: Rescale SVG stroke dashes for pathLength
patch: 0001-Rescale-SVG-stroke-dashes-for-pathLength.patch
upstream-fit: yes
source: kitesurf scripts/patches/usvg-0.48.1/0003-Implement-pathLength-rescale-stroke-dash-pattern-by-.patch
---

Rescale stroke dash lengths and offsets by the ratio of actual path length
to the author's positive `pathLength`. The host render regression checks a
line whose declared length is one tenth of its geometry. This SVG behavior
is suitable for usvg upstream.

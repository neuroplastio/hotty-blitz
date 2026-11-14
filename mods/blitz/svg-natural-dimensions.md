---
title: Size SVG images from CSS natural dimensions
patch: 0076-Size-SVG-images-from-CSS-natural-dimensions.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0021-Size-SVG-images-from-their-CSS-natural-dimensions.patch
---

Preserve the SVG's declared natural dimensions and viewBox ratio, handle zero or invalid root sizes, and normalize the tree viewport to CSS natural dimensions. Root CSS length declarations override presentation attributes; the pinned usvg otherwise ignores those declarations for its root viewport, so recognized lengths are reflected into the parser input. Parser and host tests cover missing, zero, negative, percentage, physical-unit and style-override dimensions.

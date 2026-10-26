---
title: "text-overflow draws its ellipsis"
patch: 0041-Implement-text-overflow-ellipsis.patch
upstream-fit: yes
---

**What it changes:** an inline root that clips its overflow draws
`text-overflow`'s ellipsis, or its string, at the end of each line that
runs past the content box. The line keeps its clusters and inline-blocks
up to the last one that leaves room for the ellipsis, and the ellipsis
sits right after it on the line's baseline. Inline layout shapes the
ellipsis in the root's style, and painting draws the line under a clip
that ends there. Stylo already computed the property, but nothing drew
it, so such a line was cut mid-glyph at the edge. A line overflows only
past the box's unrounded edge too, since rounding to whole pixels can
leave a box sized to its line a pixel short of it. Right-to-left lines
still clip without one.

**Why we carry it:** the UI kit keeps one-line labels with `nowrap`,
`overflow: hidden` and `text-overflow: ellipsis` in about ten places: the
picker's names, descriptions and status, a select's value, file names,
hint tips and chart labels.

**For upstream:** yes. Upstream has no `text-overflow` either. The patch
sits on our set: it passes `stroke_text` the selection flag that our
`::selection` patch adds, which upstream would drop.

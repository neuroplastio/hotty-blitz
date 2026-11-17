---
title: A table's captions are laid out above and below its grid
patch: 0096-blitz-dom-blitz-paint-lay-a-table-s-captions-out-abo.patch
upstream-fit: yes
---

Blitz built a table's grid from its rows and cells and passed a
`table-caption` child over, so a `<caption>` was neither laid out nor
drawn, whichever side it was on. CSS 2.2 §17.4 puts a table's captions in
the table wrapper box, above the grid for `caption-side: top` and below it
for `bottom`, each as wide as the table.

The table context now collects the table's captions (not absolutely
positioned ones) and makes them layout children of the table, outside the
grid. After the grid is laid out, `layout_captions` lays each caption out
at the table's border-box width less its margins and stacks them above and
below the grid. The cells and the grid's row positions, which the collapsed
borders are painted from, move down by the captions' height above. The
table's size and baselines grow by it, and a static position inside the
table moves down too. A height given to the table is taken as its whole
box's, so the grid gets what the captions leave. Paint draws the table's
own background and border around the grid alone.

Blitz has no separate table wrapper box, so a table's margins stay around
captions and grid together, as the wrapper's would. A caption whose
min-content width is wider than the table does not yet widen it, and
`overflow` clipping on the table would clip its captions.

We carry it for hotty-surf's kit, whose data tables have captions. Upstream
should want it: captions are a basic part of HTML tables, and Chromium and
Firefox lay them out this way.

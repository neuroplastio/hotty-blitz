---
title: An auto-layout table shares its spare width in proportion to its columns
patch: 0005-Share-an-auto-layout-table-s-spare-width-in-proporti.patch
upstream-fit: maybe
---

**What it changes:** a grid shares free width equally between its auto tracks
(maximise tracks, then stretch auto tracks). An auto-layout table's columns
do not share it so: between the sum of their minimum content widths and the
sum of their maximum, a column gets a share in proportion to its maximum less
its minimum, and beyond the sum of maximum, in proportion to its maximum, as
Chromium and Firefox do. Two columns of 100px and 300px content in a 1000px
table are 250px and 750px wide, not 400px and 600px. An embedder that lays a
table out as a grid says so with `GridContainerStyle::is_table_grid`
(`Style`'s `item_is_table`); the inline axis is shared that way. Other grids
and the block axis are as before.

**Why we carry it:** sorting or filtering a table changes its columns' content,
and with equal shares each column moved far more than in a browser. A
shadcn table in a wide terminal window was visibly different from the same
page in a browser.

**For upstream:** maybe. It is what browsers do for tables, but Taffy has no
table layout of its own and the hook is a method only a table embedder sets.

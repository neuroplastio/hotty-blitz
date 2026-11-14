---
title: Keep table captions out of anonymous grid cells
patch: 0082-Keep-table-captions-out-of-anonymous-cells.patch
upstream-fit: yes
---

Captions need their own table layout, which this engine does not yet implement. The anonymous-table fix must not place them as an ordinary grid cell; retain the previous omission until caption layout is implemented. A regression checks that adding a caption does not insert a row into the table grid.

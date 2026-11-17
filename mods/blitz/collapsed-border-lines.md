---
title: Collapsed table borders are resolved line by line
patch: 0098-blitz-dom-blitz-paint-resolve-a-collapsed-table-s-bo.patch
upstream-fit: yes
---

Blitz laid a `border-collapse: collapse` table out with one uniform gutter,
as wide as the first cell's widest border, and painted it in that cell's
colour. Every other cell's border, and every row's, row group's and
column's, was ignored: Tailwind's `border-b` on a `tr` drew nothing and
took no room, and a table whose cells had different borders came out
wrong.

The patch resolves each segment of each line between grid slots, and
around the grid, as CSS 2.2 §17.6.2.1 says. The candidates are the borders
of the cells, rows, row groups, columns and column groups that meet there,
and the table's. `hidden` wins outright, then the wider, the higher-priority
style and the higher-ranked box. As in Chromium, each cell's border is half
of the widest segment on each of its sides and the table's half of its
widest outer line, and the table has no padding. The cells' layout style
reads these through `TaffyStyloStyle::collapsed_border`. The table paints
each segment centred on the cells' edge, from their painted layouts, one
fill per colour. A cell paints its background in its padding box and no
border, and the table none of its own. Columns run in the table's
`direction`. This replaces the gutter painting with `PhysicalTracks`.

It also fixes the column cursor, which freed a rowspan cell's columns one
row early, so a cell beside it was given the wrong column.

We carry it for hotty-surf's kit: its tables separate rows with a row's
bottom border, which Blitz neither drew nor made room for. Upstream should
want it. Not done: a cell's explicit width does not count its half borders,
and corners take the colour drawn last, not the winner's.

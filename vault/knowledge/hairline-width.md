# An inset hairline's width at a fractional scale

`inset 0 0 0 1px` is 1 device pixel at 1.6 (mods/blitz 0051): the spread
is rounded down to whole device pixels, at least one, as Stylo snaps a
border's width, so a ring sits on the grid of a 1px border or focus
outline beside it. The complaint (2026-11-04) was uneven sides, not
weight. Rounding to nearest, 2 device pixels at 1.6, was tried the same
day: even too, but a focused control looked bold, and hotty-surf's gate
against Chromium failed 260 of 702 cases against 242 rounding down.
Chromium itself keeps the 1.6 px, anti-aliased (1.05 to 1.6 px of ink a
side by position).

Snapping the painted box (the same patch) turns a sub-pixel gap between
Blitz's layout, on whole CSS px, and Chromium's into a whole device row
where an edge rounds the other way: hotty-surf's text/clickable dense
1.6 fails on its last row for that, nothing in the drawing itself.

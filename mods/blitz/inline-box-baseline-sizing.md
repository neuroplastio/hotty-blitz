---
title: An inline flex, grid or table box is laid out for its baseline when the line is sized
patch: 0094-blitz-dom-an-inline-flex-grid-or-table-box-is-laid-o.patch
upstream-fit: yes
---

An inline box's baseline decides where the line puts it, so it is part of
the height the line gets. When a line was only being sized (a block that is
a flex item, measured by its flex container in ComputeSize mode), Blitz's
inline layout passed that mode on to its inline boxes. Taffy computes a
flex, grid or table container's baselines only in a full layout, so such a
box came back with none and sat on its bottom edge. A column flex holding a
div with an inline-flex button in it measured the div about 6 px too tall,
leaving a strip under the button that the final layout no longer
explained. The same div as a block was right, since block layout lays its
children out in full.

The patch lays an inline box that is not a flow container out in full
while the line is sized as well, when the box exports a baseline. A flow
inline-block is left as it was: block layout gives its baseline in
ComputeSize already.

We carry it for hotty-surf's kit: a button in its own div inside a flex
column (Basecoat's layouts, its dialog) sat several pixels low, with the
div's background showing under it. Upstream should want it. It costs a full
layout of such boxes during measurement, which the final pass's cache then
reuses when its inputs are the same.

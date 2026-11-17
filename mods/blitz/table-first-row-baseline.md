---
title: A table's baseline is its first row's
patch: 0099-blitz-dom-a-table-s-baseline-is-its-first-row-s.patch
upstream-fit: yes
---

Blitz gave a table the baseline Taffy's grid gives it: its first cell's
first baseline, the baseline of that cell's text. CSS 2.2 §17.5.3 gives a
table the baseline of its first row. When no cell in that row is aligned on
its baseline, that is the bottom of the content box of the row's lowest
cell. A cell's `vertical-align` is `middle` by default, inherited from the
UA sheet's row groups, so that is the usual case. An inline table, or a
table in a flex row aligned on baselines, sat about a quarter of a line
higher next to text than in Chromium and Firefox.

`first_row_baseline` gives that edge, from the cells that start in the
first row and span only it (or every cell in it, when they all span
further), once the grid is laid out. When a cell of the row is aligned on
its baseline, or is an implicit cell around a box that is not one, the
grid's baseline stands as before. Captions move the baseline down with the
grid.

We carry it for hotty-surf's kit, whose tables and table-like controls
sit in lines and baseline rows next to text. Upstream should want it: it is
CSS 2.2's rule, and Chromium and Firefox both follow it.

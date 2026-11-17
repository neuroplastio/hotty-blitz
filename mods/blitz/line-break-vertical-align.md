---
title: "Use the line break's vertical alignment"
patch: 0101-blitz-dom-use-line-break-vertical-alignment.patch
upstream-fit: yes
---

A `br` produces its preserved newline with its own computed vertical alignment
instead of inheriting the inline context root's alignment. `vertical-align` is
not inherited: on a table cell it places the cell's contents and must not be
applied again to the line break inside them. With the cell's default middle
alignment, the inherited value enlarged the first line box and made a two-line
cell 2 CSS pixels taller than in Chromium. This also honors an alignment set
on the break itself.

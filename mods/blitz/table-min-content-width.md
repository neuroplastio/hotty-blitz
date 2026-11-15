---
title: "An auto-layout table is not narrower than its minimum content"
patch: 0089-blitz-dom-an-auto-layout-table-is-not-narrower-than-.patch
upstream-fit: yes
---

**What it changes:** the used width of a `table-layout: auto` table is the
greater of its `width` and its minimum content width (CSS 2.1 section
17.5.2.2). Blitz sized the table's grid at `width` alone, so a `width: 100%`
table in a narrow wrapper was as wide as the wrapper, its columns overflowed
the table box, and, as a table's scrollable overflow is capped at its own
size, the wrapper had nothing to scroll. When `width` resolves to a definite
size, the grid is now measured under a min-content constraint, and that is
the known width if it is larger. A `table-layout: fixed` table with a width
is left alone.

**Why we carry it:** shadcn's Table is a `w-full` table in an
`overflow-x: auto` wrapper. In a narrow terminal window the columns that did
not fit were cut off and no wheel or trackpad could reach them.

**For upstream:** yes. It is a CSS conformance fix that any page with a
`width: 100%` table can hit. It costs one extra min-content measurement for
a table with a definite width.

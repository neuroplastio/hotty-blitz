---
title: "A font query keeps what the last one matched"
patch: 0004-fontique-a-query-keeps-what-the-last-one-matched.patch
upstream-fit: maybe
---

**What it changes:** Parley makes a fontique query for every text layout
it shapes, and another for the layout's line metrics. Each query cleared
the collection's query state first, so every layout listed its font
stack's families again, looked them up, matched them and loaded their
fonts, then dropped it all. The state now outlives the query: the same
families keep their matches, the same fallback key keeps its fallbacks,
and fonts are matched again only for other attributes. A query that
sets no families, or no fallbacks, still matches none. Whatever changes
the collection, or a shared collection's new version, forgets the state.

**Why we carry it:** plx draws a dialog's box as one inline block per
cell, each a text layout of its own: about 1,340 for the palette's
60 by 22 box, a new document each time the palette opens. In
`hotty replay` of that box (gov R-22), building those layouts took 9 of
the 11 ms of Blitz's resolve, and 3.5 of 5.3 with this. Through plx in
hottyterm, the box's resolve on opening the palette went from 16.7 ms
to 8.5 (p50, n=12).

**Platform:** neutral. The query sits above fontique's backends, so it
is the same on macOS (CoreText) as on fontconfig.

**For upstream:** maybe. Nothing a caller sees changes, and the tests
cover a query after another's attributes, one that names no family, and
fonts registered between two queries. Upstream might rather cache the
match per font stack in Parley's layout context.

---
title: Generate anonymous table objects
patch: 0078-Generate-anonymous-table-objects.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0028-Anonymous-table-objects-CSS-2.2-17.2.1-both-directio.patch
---

Wrap runs of table-internal boxes in an anonymous table and put stray cells in implicit rows. Retain non-table element content inside tables. Refresh synthetic-table styles on incremental restyles; traverse display:contents transparently. Layout and delta tests cover adjacent cells, ordinary element content, parent restyles, removal, hiding and rebuilding. Direct bare text, table caption layout and complete CSS table fixup remain outside this patch.

---
title: Draw collapsed table borders per cell
patch: 0080-Draw-collapsed-table-borders-per-cell.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-paint-0.3.0-beta.4/0012-Draw-collapsed-table-borders-per-cell-not-per-gridli.patch
---

Use the detailed grid items to paint individual cell edges, rather than sweeping each gutter across rows and columns. This avoids drawing through spanning cells while retaining the table's authored border band. Pixel tests cover colspan and rowspan. The companion DOM 0023 storage is already provided by upstream Blitz 813c496bf and its resolved-style consumer 33af534a5, so it is not ported.

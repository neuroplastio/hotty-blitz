---
title: "A measurement from the cache keeps its baselines"
patch: 0037-blitz-dom-keep-baselines-in-measure-cache-entries.patch
upstream-fit: yes
---

**What it changes:** a measure entry in the layout cache keeps the
baselines of its answer as well as the size, and a hit answers with both.
Before, a hit had no baselines, so inline layout sat an inline-block
measured from the cache on the line by its bottom edge and the line grew.
A wrapping flex row inside a column measures an item a third time under
constraints it has used before, so an item of keycaps sized at 25.4px
came back 30.1px, and the text beside it sat 4px lower than in the rows
around it.

**Why we carry it:** the UI kit's cheatsheet is a column of such rows,
and its "New tab" row was the one whose keys hit the cache.

**For upstream:** yes. The patch sits on our layout cache (0001), but
taffy's `Cache`, which upstream uses, drops baselines from measure
entries the same way, and upstream's inline layout reads them from there.
The same change to taffy's `Cache` would fix it.

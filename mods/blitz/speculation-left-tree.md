---
title: "Speculative relayout leaves nodes out of the layout tree alone"
patch: 0026-blitz-dom-speculative-relayout-leaves-nodes-out-of-t.patch
upstream-fit: maybe
---

**What it changes:** the speculative relayout of "A delta costs the change,
not the document" asked nodes that had just left the layout tree about
their layout parent, and laid out their children, both possibly freed. It
now passes over a node unless each layout parent up to the root element
lists it; its old container lays out again, as for any unsettled child.

**Why we carry it:** a crash on freed nodes, found by hotty-blitz's fuzz
test.

**For upstream:** maybe, as part of "A delta costs the change, not the
document": it fixes that patch, and goes wherever it goes.

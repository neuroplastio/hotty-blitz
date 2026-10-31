---
title: "A delta costs the change, not the document"
patch: 0001-blitz-dom-patch-cost-proportional-to-depth-not-docum.patch
upstream-fit: maybe
---

**What it changes:** one changed node cost every phase of `resolve` a walk
of the whole document. Each phase now stops where the change stops.
Construction skips elements with no damage below them. Each node's layout
cache grows past taffy's 9 entries, up to 64, since a content-sized
container k levels deep is measured under about 3 + 5k constraints a pass
and the entries evicted each other. A flex or grid container keeps its
cache when every changed item, asked again what the container asked it
last time, answers the same (`layout/speculate.rs`). Rounding visits only
the nodes whose layout changed and the subtrees that moved, and everything
when more than a quarter changed. Debug builds compare every incremental
rounding with `taffy::round_layout`, and the tests of Blitz's DOM, HTML
and paint crates pass. With Blitz's `writing-mode` feature, which
hotty-blitz doesn't build, layout and rounding stay upstream's.

**Why we carry it:** hotty programs send deltas many times a second; a
one-cell change to a large dashboard cost the whole document.

**For upstream:** maybe. The cache and the rounding are generic, but the
speculation is a large change to how layout settles, and the
`writing-mode` path would need its own incremental rounding.

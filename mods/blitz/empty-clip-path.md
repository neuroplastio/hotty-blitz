---
title: Treat empty clip paths as empty regions
patch: 0068-Treat-empty-clip-paths-as-empty-regions.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-paint-0.3.0-beta.4/0010-Handle-empty-clip-regions-and-the-layer-budget-corre.patch
---

An inset clip whose opposing edges meet or cross clips the subtree away,
rather than behaving as though there were no clip. Skip the empty-clipped
subtree, preserving the existing outside outline and shadow order. Render
tests cover meeting, crossing and nonempty insets. The source patch's layer
budget correction is already carried by layer-depth.md. The clip fix suits
upstream.

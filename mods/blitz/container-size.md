---
title: "Resolve container queries against laid-out container sizes"
patch: 0048-Resolve-container-queries-against-laid-out-container.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0020-Implement-query_container_size-from-the-last-taffy-l.patch; kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0048-Restyle-container-queries-against-laid-out-container.patch
---

Provide Stylo the last content-box size of query containers and restyle containers whose sizes change after layout. Layout and restyle repeat up to eight passes so nested queries settle without blocking a resolve indefinitely. Hotty needs CSS container queries and units; this integration belongs upstream. Adapted from kitesurf (Apache-2.0).

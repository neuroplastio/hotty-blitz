---
title: Provide the layout size to container queries
patch: 0065-Implement-query_container_size-from-the-last-taffy-l.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0020-Implement-query_container_size-from-the-last-taffy-l.patch
---

Supply Stylo the last layout's content-box size for query containers.
This is necessary for container queries and units and could go upstream
with a way to settle queries after layout. From kitesurf (Apache-2.0).

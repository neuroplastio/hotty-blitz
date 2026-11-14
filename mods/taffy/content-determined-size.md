---
title: Size content-determined containers from non-cyclic content
patch: 0003-Size-content-determined-containers.patch
upstream-fit: yes
source: kitesurf scripts/patches/taffy-0.15.0/0005-Size-content-determined-containers-from-their-non-c.patch
---

Pass the definite content-box dimensions into block intrinsic measurement, recollect wrapping flex lines once their main size is resolved, and include the content-box inset when returning an outer stretch-fit size. A red/green host test covers a padded wrapping inline-flex. Upstream 4aca4f49 already implements the replaced percentage min-content cap, including max-width; keep that implementation.

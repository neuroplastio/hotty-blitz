---
title: "A render context takes at most 4 workers by default"
patch: 0002-vello_cpu-a-render-context-takes-at-most-4-workers-b.patch
upstream-fit: no
---

**What it changes:** the default thread count was one worker per core but
one, up to 8; it is now up to 4. Measured with a 300-frame dashboard replay
and hotty-blitz's benchmark on a Ryzen 5 5600X and a laptop: past 2
workers a full paint barely gets faster, since the main thread's share
dominates, while each worker adds dispatch and idle spinning to every
small paint. With 4, replay paint time fell about 16% and CPU time about
35% on the 5600X, and a full paint stayed within a millisecond.

**Why we carry it:** HOTTY repaints are mostly small, and a host keeps 8
render contexts, so this halves its threads, from 64 to 32. The
maintainer chose 4 (2026-10-10).

**For upstream:** no. The number is tuned to our workload, and the
maintainer decided not to offer these patches.

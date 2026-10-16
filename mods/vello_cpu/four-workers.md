---
title: "A render context takes at most 4 workers by default"
patch: 0002-vello_cpu-a-render-context-takes-at-most-4-workers-b.patch
upstream-fit: maybe
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

**For upstream:** maybe. The measurement is worth having: on two machines,
past 2 workers a full paint barely gets faster, and every worker costs
each small paint. But the right default depends on what a renderer
paints, upstream's benchmarks may differ, and a caller can already set
`num_threads`. vello_cpu 0.3.0 still takes up to 8.

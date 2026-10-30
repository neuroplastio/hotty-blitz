---
title: "A small render context draws without workers"
patch: 0004-vello_cpu-a-small-render-context-draws-without-worke.patch
upstream-fit: maybe
---

**What it changes:** `RenderContext::new` gives a context of fewer than
256 K pixels no workers (`num_threads` 0), so it draws on the calling
thread; a larger one still takes up to 4 (0002). `new_with` takes any
count, as before.

**Why we carry it:** hotty-blitz keeps a render context per paint size,
up to 10, at most 4 of them large, and a terminal's paints are mostly
small: plx's pane ring paints strips such as 2432×64. Each small context
kept 4 idle workers, and a paint that small is over before they have
taken much of it. With none, a host keeps 16 worker threads at most
instead of 40. In `hotty replay` of plx's palette as plx sends it (gov
R-22, n=216, p50), the box's paint went from 2.69 to 2.31 ms and the
chrome's from 0.12 to 0.07; through plx in hottyterm, a paint's p50 went
from 0.70 to 0.45 ms (n=1362), with E6's full-surface paints unchanged.
No workers for every context, or one, cost those: a 2385×1485 paint's
raster went from 1.25 to 1.8 ms.

**For upstream:** maybe, for the same reason as 0002: the right default
depends on what a caller paints, and a caller can already set
`num_threads`. The line at 256 K pixels is where hotty-blitz splits its
small contexts from its large ones. The patch applies to vello_cpu 0.2.0
and 0.3.0, and the renderer is the same on every platform.

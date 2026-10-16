---
title: "Flushing waits for the workers without spinning"
patch: 0001-vello_cpu-flushing-waits-for-the-workers-without-spi.patch
upstream-fit: yes
---

**What it changes:** `MultiThreadedDispatcher::flush` drained the workers'
recorded commands by looping on `try_recv` until every worker had hung up,
so the painting thread spun at full speed for as long as the workers took
on the last batch. It now blocks in the ordered channel's `recv`, which
hands out the tasks in the same order and fails once the senders are gone
and nothing is buffered. Counting instructions with `perf` on a 10 Hz
dashboard in hottyterm: the painting thread did 39% fewer, the process 23%
fewer, and frame times stayed the same.

**Why we carry it:** a quarter of hotty-blitz's instructions on that
dashboard were this spin.

**For upstream:** yes. It is a fix with no trade-off: the order and the
results are the same, and only the busy wait goes. vello_cpu 0.3.0 still
drains this way, and the patch applies there.

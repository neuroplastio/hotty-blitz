---
title: "The layer limit counts the layers open at once"
patch: 0041-blitz-paint-limit-the-layers-open-at-once-not-the-la.patch
upstream-fit: maybe
---

**What it changes:** blitz-paint's `LayerManager` pushes a clip layer for
each element that clips, and stops at `LAYER_LIMIT`, 1,024. It counted
every layer pushed in a paint and never took one off when it was popped,
so a document with more than 1,024 clipped elements side by side lost the
clips of every element after the 1,024th, and their content painted past
their boxes. The check is now on the layers open at once, which bounds
nesting as before.

**Why we carry it:** a hotty program's surface can hold many small
clipped boxes, such as one per cell of a grid. plx's dialog box had about
1,340 and lost its clips from row 16 down. `tests/clips.rs` paints 1,100
clipped boxes and checks every one clips. The clips it no longer drops
cost what clips cost: plx's box paints in 2.91 ms a frame, from 2.82, and
plx's palette opens as before.

**Platform:** neutral. It is Blitz code only, the same on macOS.

**For upstream:** maybe. The limit came with clipped rendering, as a
workaround for vello's GPU renderer, where a deep layer stack fails.
Counting the open layers keeps that bound and stops dropping clips that
never stack. Upstream might rather make the limit the renderer's, since
vello_cpu, which hotty-blitz paints with, has none.

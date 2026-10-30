---
title: "A clip layer nothing is drawn in is not pushed"
patch: 0005-vello_cpu-a-clip-layer-nothing-is-drawn-in-is-not-pu.patch
upstream-fit: yes
---

**What it changes:** `push_clip_layer` keeps the layer back, and the
render context gives it to the dispatcher just before the first draw
inside it. A clip layer popped before anything is drawn in it never
reaches the dispatcher. A fill or stroke of an empty path doesn't count
as a draw. Another layer, or a clip path pushed or popped, pushes
whatever is held back first, so the dispatcher sees the same layers in
the same order and under the same clip paths, less the empty ones.

**Why we carry it:** a clip layer with nothing in it changes no pixel,
but pushing one still costs the clip path's strips and the layer's
bookkeeping. Blitz pushes one for every `overflow: hidden` element
before it knows whether anything inside will draw. plx draws its box as
a grid of cells, each an `overflow: hidden` element so that a glyph
wider than its cell is cut, and most of a terminal's cells are blank:
1,366 clip layers a frame, nearly all of them empty. In `hotty replay`
of plx's palette and box (gov R-22, pinned, n=576, p50), the box's scene
went from 1.19 to 0.90 ms and its paint from 2.28 to 2.05. Through plx in
hottyterm, the box's paint went from 3.28 to 2.59 ms. The pixels are the
same: the cases tested render to the same bytes, with workers and
without.

**For upstream:** yes. It is a change inside `RenderContext`, under the
same API, and any renderer behind a DOM gets clip layers that stay
empty. vello_hybrid could do the same. The patch applies to vello_cpu
0.1.0 and 0.2.0. 0.3.0 adds `push_clip_rect` and renames the clip
path's pop to `pop_clip`, which need the same hook, so it takes a
version of its own there. The renderer is the same on every platform.

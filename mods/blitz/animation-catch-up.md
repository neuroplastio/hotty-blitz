---
title: "An animation catches up every iteration a gap skipped"
patch: 0110-blitz-dom-an-animation-catches-up-every-iteration-a-.patch
upstream-fit: yes
---

**What it changes:** where `resolve_stylist` ticks the document's CSS
animations, each one iterates until the iteration in progress is the one
the resolve's time falls in, not by one iteration at most.

**Why we carry it:** hotty-blitz plays infinite animations from memory
once it has seen them through a period (`loops.rs`), and resolves nothing
meanwhile. The first resolve after that found each animation iterations
behind: Stylo's progress clamps to the end of the current iteration, so
the animation showed its last keyframe and caught up by one iteration a
frame. `tests/loops.rs` paints after three periods from memory and fails
without it.

**For upstream:** yes. Any embedder that resolves less often than an
animation iterates hits it: a window in the background, a frame that
stalls longer than a short spinner's turn. It is one loop; each step is
Stylo's own `iterate_if_necessary`.

---
title: "A scroll offset comes back into range after layout"
patch: 0106-blitz-dom-a-scroll-offset-comes-back-into-range-afte.patch
upstream-fit: yes
---

**What it changes:** after layout, `resolve` brings every scroll offset
that is now out of range back into it, as a browser does. A box or a
viewport that grew, or whose content shrank, has less to scroll than
before. It now shows its new end, or its top when everything fits,
instead of the old offset. Only the boxes whose layout changed are
checked (`geometry_changed`), plus the viewport, whose range also changes
with the window. The offset moves the way layout moves it: no scrollbar
shows for it. An `overflow: hidden` box keeps an offset a script gave it,
as far as it still reaches. `take_clamped_scrollers` lists what moved,
with the root element standing for the viewport.

**Why we carry it:** Blitz clamped an offset only when something
scrolled. Shrink a terminal window until a box scrolls, scroll it down,
then grow the window again. The scrollbar went away, but the box stayed
scrolled, with its top cut off and nothing that could scroll it back. A
delta that removed rows from a scrolled box did the same. hotty-blitz
paints only what changed, so it takes the list to paint each moved box
again. A box with `scroll-snap-type` then comes to rest at a snap
position, unless a gesture is still moving it. `tests/scroll.rs` covers
the root and an inner box, both on a resize and on a delta.

**For upstream:** yes. Any embedder that resizes its window has this
bug. The list is only for embedders that track their own damage, like
`take_animated_nodes`. Not done: a `scroll` event for the move (Blitz
sends none for programmatic scrolls yet), and snapping again after
layout, which is left to the embedder, since only it knows when a
gesture ends.

**Platform:** neutral. It is Blitz code only, the same on macOS.

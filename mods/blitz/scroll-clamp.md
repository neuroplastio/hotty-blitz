---
title: "A scroll offset comes back into range after layout"
patch: 0106-blitz-dom-a-scroll-offset-comes-back-into-range-afte.patch
upstream-fit: yes
---

**What it changes:** after layout, `resolve` brings every scroll offset
that is now out of range back into it, as a browser does. A box or a
viewport that grew, or whose content shrank, now shows its new end, or
its top when everything fits, instead of the old offset. Only the
viewport and the boxes whose layout changed are checked. Those boxes are
`geometry_changed`, which overflow-after-relayout.md adds, so this patch
builds on that one and doesn't apply to Blitz alone. The offset moves
the way layout moves it, so no scrollbar shows. An `overflow: hidden`
box keeps an offset a script gave it, as far as it still reaches.
`take_clamped_scrollers` lists what moved, with the root element
standing for the viewport. `hold_scroll_clamp` holds the clamp for a
layout at a size that is never shown, and the boxes laid out then are
checked at the next resolve.

**Why we carry it:** Blitz clamped an offset only when something
scrolled. Shrink a terminal window until a box scrolls, scroll it down,
then grow the window again. The scrollbar went away, but the box stayed
scrolled, its top cut off. A delta that removed rows did the same.
hotty-blitz paints only what changed, so it takes the list. A box with
`scroll-snap-type` then comes to rest at a snap position, unless a
gesture still holds it. hotty-blitz measures `r=auto` content one pixel
tall, where a box whose rows are `vh` has nothing to scroll, so it holds
the clamp for that layout. `tests/scroll.rs` and `tests/snap.rs` cover
it.

**For upstream:** yes. Any embedder that resizes its window has this
bug. The list and the hold are for embedders that track their own
damage or measure content, like `take_animated_nodes`. Not done: a
`scroll` event for the move (Blitz sends none for programmatic scrolls
yet), and snapping after layout, which is left to the embedder, since
only it knows when a gesture ends.

**Platform:** neutral. It is Blitz code only, the same on macOS.

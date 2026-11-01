---
title: "An embedder hears which elements' animated styles moved"
patch: 0049-blitz-dom-an-embedder-hears-which-elements-animated-.patch
upstream-fit: maybe
---

**What it changes:** the style traversal collects the elements whose
computed style moved under a CSS animation or transition (restyled this
pass, `may_have_animations` set), with the `::before`/`::after` boxes
their style carries, and `BaseDocument::take_animated_nodes` hands the
list over, emptied. `has_active_animations` says whether one is still
mid-flight, apart from `is_animating`'s scroll animations, fading
scrollbars and `<canvas>`.

**Why we carry it:** hotty-blitz repaints rectangles of its own choosing
into a caller's buffer, and its damage tracker is fed where a mutation
happens. An animation moves a style with no mutation, and Blitz's restyle
damage holds layout only, so a colour moving under a transition would
never be reported and the frame would repaint nothing. With the list, the
host touches those nodes as it touches a delta's, and the old and new
extents give the rectangles.

**For upstream:** maybe. Blitz's own shell repaints whole frames, so it
has no use for the list; any embedder with its own damage tracker does.
The list costs one `push` per animated restyle, nothing for documents
without animations.

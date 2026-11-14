---
title: "The document says when an animation's output next changes"
patch: 0053-blitz-dom-the-document-says-when-an-animation-s-outp.patch
upstream-fit: maybe
---

**What it changes:** `resolve_stylist` keeps the earliest of Stylo's
`ElementAnimationSet::next_change` over the document's animation sets,
next to `has_active_animations`, and
`BaseDocument::next_animation_change()` returns it, on the timeline
`resolve` takes its time on: the start of an animation in its delay, the
next jump of a `steps()` easing, or the time of that resolve while an
easing moves continuously. `None` when none will change.

**Why we carry it:** hotty-blitz tells its host when the next frame is
due. While an animation was active that was every 60th of a second, so a
`steps()` countdown bar at ten steps a second (plx's toast) resolved the
document 60 times a second and painted 10, and an animation in its
delay resolved 60 times a second and painted nothing. hotty-blitz now
asks for a frame when the output next changes, never sooner than one
frame after the last.

**For upstream:** maybe. It needs mods/stylo's `next_change` (Stylo
`animation-next-change`). Blitz's own shell redraws while
`is_animating` holds and would gain the same by scheduling its redraw
at this time; that is its call.

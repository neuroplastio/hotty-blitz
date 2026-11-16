---
title: "Culling counts what an element paints past its box"
patch: 0090-blitz-paint-culling-counts-what-an-element-paints-pa.patch
upstream-fit: yes
---

**What it changes:** Blitz's painter culls an element whose border box and
scrollable overflow lie outside the clip rectangle. An outer `box-shadow`
and an `outline` paint past both, so an element just outside the viewport
was culled with its shadow still reaching in, and the shadow's edge went
missing. The cull box now grows by each outer shadow's offset, spread and
blur (three standard deviations, as `draw_box_shadow` blurs) and by the
outline's width and offset.

**Why we carry it:** hotty-blitz paints part of a surface by scrolling the
viewport onto the rectangle to repaint, so every partial repaint has edges
there. When a root scroll moves the painted pixels and repaints only the
strip it uncovers (hottyterm's touch scroll, 2026-11-16), the strip's top
edge cut the shadows of hotty-pot's task badges: a row of wrong pixels a
frame. hotty-blitz's `surface::tests` compare such scrolls with full
paints, shadows included.

**For upstream:** yes. It is a correctness fix for any viewport edge,
scrolled or not, at the cost of reading two styles a culled element
already has.

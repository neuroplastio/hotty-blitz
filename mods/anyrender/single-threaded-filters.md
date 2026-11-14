---
title: Allow filters in a single-threaded renderer
patch: 0001-Allow-filtered-scenes-in-single-threaded-renderers.patch
upstream-fit: yes
---

AnyRender discarded filters whenever its multithreading feature was compiled, even for a zero-worker Vello context. Retain filters when the context is single-threaded, and expose a zero-worker image renderer that stays single-threaded after resize. The host detects filters while forwarding the scene and repaints filtered scenes on that renderer; other scenes keep the normal worker-backed renderer.

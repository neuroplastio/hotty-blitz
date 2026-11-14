---
title: Lay out WebKit boxes and truncate legacy line clamps
patch: 0079-Lay-out-WebKit-boxes-and-line-clamps.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0022-webkit-box-flex-emulated-layout-line-clamp-line-trun.patch
---

Lay out unclamped legacy WebKit boxes as flex containers with their requested orientation. When Stylo adjusts the legacy vertical clamp to a flow root, truncate its Parley lines. This ports truncation only; it does not add an ellipsis glyph. A host regression checks both box orientations, clamp changes and that a plain block is not clamped.

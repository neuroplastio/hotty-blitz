---
title: Resolve CSS counters and update changed markers
patch: 0081-Resolve-CSS-counters-and-update-changed-markers.patch
upstream-fit: maybe
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0027-Implement-CSS-counters-resolution-pass-counter-conte.patch; scripts/patches/blitz-dom-0.3.0-beta.4/0041-Damage-the-nodes-whose-counters-moved-since-the-last.patch
---

Resolve list-item counters and generated `counter()`/`counters()` content after styling, and reconstruct owners whose marker or pseudo text changed. Repeat the pass after container-query restyles. This base has no shadow DOM, so it traverses DOM children; the source patch's shadow-boundary counter scoping remains unimplemented. Tests cover ordered starts and values, reversed lists, zero, generated content and non-local renumbering after deltas. Existing marker-format tests also pass.

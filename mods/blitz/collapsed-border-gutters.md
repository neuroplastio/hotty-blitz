---
title: Place shared collapsed borders in track gutters
patch: 0084-Place-shared-collapsed-borders-in-track-gutters.patch
upstream-fit: yes
---

Both cells adjoining a shared border must paint the same gutter. Painting one edge inside the preceding track doubled the visible border width on this Taffy base. Pixel tests check the authored width in both axes, including spanning cells.

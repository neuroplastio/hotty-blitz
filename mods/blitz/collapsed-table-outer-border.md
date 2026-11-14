---
title: Preserve the outer collapsed border from cells
patch: 0083-Preserve-the-outer-collapsed-border-from-cells.patch
upstream-fit: yes
---

This Taffy base represents the collapsed frame supplied by cell borders in track gutters rather than `layout.border`. Use those gutters for the outer frame, retaining the existing authored-table-border painter. Pixel tests cover the frame as well as colspan and rowspan interior edges.

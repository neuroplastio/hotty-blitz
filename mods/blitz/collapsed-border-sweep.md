---
title: Sweep active collapsed border edges by row
patch: 0087-Sweep-active-collapsed-border-edges-by-row.patch
upstream-fit: yes
---

The union introduced for collapsed cell borders scanned every rectangle for every horizontal band. Sort rectangles once by their start and maintain only those active in each band, reusing the span buffers. Locate grid boundaries by binary search instead of scanning tracks for each cell. Geometry and antialiasing match the previous painter. hotty-perf measured the 300×8 table at 4.27 ms paint p50 against 2.97 ms on 0.7.5 (+45%, down from +229%); 04-table was level. Gov accepted this for release, with further large-table optimization as a follow-up.

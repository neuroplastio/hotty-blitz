---
title: Sweep active collapsed border edges by row
patch: 0087-Sweep-active-collapsed-border-edges-by-row.patch
upstream-fit: yes
---

The union introduced for collapsed cell borders scanned every rectangle for every horizontal band. Sort rectangles once by their start and maintain only those active in each band, reusing the span buffers. Locate grid boundaries by binary search instead of scanning tracks for each cell. The resulting geometry and antialiasing must match the previous painter; hotty-perf measures the large 300-row fixture against 0.7.5 before shipping.

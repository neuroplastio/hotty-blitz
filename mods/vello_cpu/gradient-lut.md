---
title: Reuse consecutive gradient lookup tables
patch: 0006-Share-one-gradient-LUT-across-consecutive-equal-grad.patch
upstream-fit: maybe
source: kitesurf scripts/patches/vello_common-0.3.0/0001-Share-one-gradient-LUT-across-consecutive-equal-grad.patch
---

Reuse the gradient lookup table for consecutive equal gradients, avoiding
repeated allocation for tiled patterns. The R-33 same-build controlled rig
measured a radial dot grid 30% faster (6.1 to 4.3 ms), with identical
pixels across all 41 documents. See vault/bench/r33/README.md. From kitesurf
(Apache-2.0).

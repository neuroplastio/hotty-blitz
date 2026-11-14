---
title: Fix SIMD argument order in blurred rectangle tests
patch: 0007-Fix-SIMD-argument-order-in-blurred-rectangle-tests.patch
upstream-fit: no
---

Update the carried interior-shortcut tests for vello_cpu 0.4, whose SIMD
constructor takes the backend first. Hotty needs these tests to verify its
blurred-rectangle optimization. The change only fixes our own test code.

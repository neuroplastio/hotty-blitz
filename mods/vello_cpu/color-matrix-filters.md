---
title: Apply color-matrix filter passes on the CPU
patch: 0008-Apply-color-matrix-filter-passes-on-the-CPU.patch
upstream-fit: yes
---

Execute color matrices over the rendered layer on both CPU precision paths. Convert premultiplied pixels to straight RGBA, apply and clamp the 4x5 matrix, and premultiply again. Matrix-only graphs run each primitive in order so clamping between passes is preserved. The fork test pins translucent and transparent pixels; host tests cover brightness, invert and clamped chains.

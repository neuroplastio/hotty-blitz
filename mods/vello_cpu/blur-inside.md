---
title: "A blurred rectangle's inside is not worked out pixel by pixel"
patch: 0003-vello_cpu-a-blurred-rectangle-s-inside-is-not-worked.patch
upstream-fit: yes
---

**What it changes:** `fill_blurred_rounded_rect` (a box shadow) paints
its whole rectangle with a shader that works out each pixel's alpha with
three scalar `powf`s and two `erf` approximations, inside the rectangle
too, where the alpha is 1. The shader now gives 1.0 straight away for a
chunk of 8 pixels whose corners are all at least `max(r1, 7σ)` and a
pixel from the edges, when `scale` is exactly 0.5. There both `erf7`
terms are ±1.0 exactly in f32, so the formula's own result is 1.0 and
the pixels are the same bits; a test compares every lane of both over
rectangles, radii, blurs and transforms.

**Why we carry it:** plx draws its dialog boxes with a shadow
(`box-shadow: 0 2px 6px`), and hotty-a2ui's fit surface, which plx's
palette is, spreads one over the whole surface (`0 0 0 100vh`), so the
shader ran over every pixel of every paint of the palette. In `hotty
replay` of the palette as plx sends it (gov R-22, n=216, p50), the
palette's raster went from 1.90 to 1.62 ms and its box's from 0.97 to
0.82; through plx in hottyterm, the palette's raster per opening went
from 3.86 to 3.43 ms (n=24 and 26).

**For upstream:** yes. The result is unchanged and the cost of a large
shadow now scales with its edge, not its area. vello_cpu 0.2.0 and 0.3.0
work the alpha out the same way, and the patch applies to both. The
renderer is the same on every platform.

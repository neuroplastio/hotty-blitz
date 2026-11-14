---
title: Speed up blurred rounded rectangles
patch: 0005-Speed-up-filling-blurred-rounded-rects.patch
upstream-fit: maybe
source: kitesurf scripts/patches/vello_cpu-0.3.0/0001-Speed-up-filling-blurred-rounded-rects.patch
---

Use a polynomial p-norm for blurred rectangle corners and skip the p-norm
outside corners. The shipping hotty-blitz dependency enables `poly-pnorm`.
The R-33 same-build controlled rig measured shadowed pages 65–74% faster:
dialogs at 1.6 scale fell from 18.9 to 5.6 ms, and 06-effects was 32%
faster. The measured tolerance is at most 1/255 per channel; the corpus
was identical. Kitesurf reports less than half an alpha LSB of polynomial
fit error. Wide-blur interpolation is omitted because overlapping shadows
exceeded the rendered 1/255 bound. See vault/bench/r33/README.md.

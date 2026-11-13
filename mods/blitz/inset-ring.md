---
title: "An unblurred inset box-shadow is one ring fill"
patch: 0047-blitz-paint-draw-an-unblurred-inset-box-shadow-as-on.patch
upstream-fit: yes
---

**What it changes:** an inset box-shadow without blur or offset, such as
`inset 0 0 0 1px`, was two layers the size of the padding box: the box
filled with the shadow's colour, then the hole cut out by a blurred
rectangle in a `DestOut` layer, blur 0 or not. Now, when the transform is
a translation, it is one even-odd fill of the ring between the padding
box and the hole. The hole's edges are snapped to device pixels, where
the unblurred rectangle's step at the pixel centres put them, and kept
inside the padding box: straight edges are pixel-identical, and the
rounded inner edge is now evenly anti-aliased, where the step made it
heavier on a 12 px corner at 1.6. A spread past half the box leaves no
hole, as CSS says; the blurred rectangle's `abs()` turned the inverted
hole into one. A spread of zero or less leaves no shadow, where the
layers left the rounded corners' anti-aliasing behind, up to 38 of 255.

**Why we carry it:** hotty-surf's look draws its hairlines this way: a
dialog, a field, a key, a button, a box. plx's palette, a dialog of
990×759 device pixels at 1.6 painted whole on each key, rastered in
1.48 ms a frame, 0.82 of it these rings; with the patch 0.66 (paired
over 94 frames, 95% interval −0.85 to −0.79 ms, gov R-22).

**For upstream:** yes. It is a cost fix with the same pixels on
straight edges, and the spread fixes are CSS's. A blurred or offset inset
shadow still takes the layers.

**Platform:** neutral. It is Blitz code only, the same on macOS.

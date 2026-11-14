---
title: Synthetic italic leans to the right
patch: 0058-Lean-synthetic-italic-to-the-right-not-the-left.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-paint-0.3.0-beta.4/0013-Lean-synthetic-italic-to-the-right-not-the-left.patch
---

Font synthesis reports slant in font coordinates, where the vertical axis
points up; the painter applies it in device coordinates, where it points
down. Negating the shear makes synthetic italics lean right and applies the
same correction to text-field placeholders. Hotty needs the expected slant
when a font has no italic face. This fixes a rendering error for all Blitz
embedders and should be useful upstream. The source patch is Apache-2.0.

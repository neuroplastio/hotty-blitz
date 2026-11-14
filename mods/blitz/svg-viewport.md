---
title: Clip SVG content to its viewport
patch: 0071-Clip-SVG-content-to-its-viewport.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-paint-0.3.0-beta.4/0004-Fix-viewport-clipping-and-scaling-of-painted-SVG.patch
---

Give embedded SVG the missing user-agent overflow default and clip SVG image contents to the replaced content box. Inline SVG may still opt into visible overflow. Without a viewBox, its units remain CSS pixels. Pixel tests cover default clipping, the author override, unscaled units and SVG images; both missing default clipping and image bleeding fail on the prior painter.

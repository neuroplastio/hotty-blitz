---
title: Overflow clipping follows each axis
patch: 0057-Clip-only-the-clipped-axis-for-one-axis-overflow-cli.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-paint-0.3.0-beta.4/0024-Clip-only-the-clipped-axis-for-one-axis-overflow-cli.patch
---

An overflow clip leaves a visible axis open when clipping the other axis,
including for descendant culling and text masks. Replaced content, text
controls and paint containment keep their own clips. Hotty documents need
content beyond the visible axis to remain visible. The same correction is
useful to Blitz embedders generally. The source patch is Apache-2.0.

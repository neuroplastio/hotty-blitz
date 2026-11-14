---
title: "Expose property-only restyle damage"
patch: 0010-Expose-property-only-restyle-damage.patch
upstream-fit: maybe
---

Expose the Servo property-level damage calculation so embedders can inspect whether a style change needs repaint or layout. Hotty uses it to check that changing a box shadow does not rebuild a box. This may be useful to other Servo embedders.

---
title: Box shadows retain their shape and background
patch: 0058-Fix-box-shadow-isolation-corner-radii-and-blur-size.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-paint-0.3.0-beta.4/0007-Fix-box-shadow-isolation-corner-radii-and-blur-size.patch
---

Inset shadow compositing isolates its destructive blend, and its offset is
scaled to device pixels. Outset shadow spread keeps each corner's radius;
blurred nonuniform corners are drawn in quadrants. The existing snapped
inset-ring path is retained for
unblurred zero-offset shadows. Hotty needs stable backgrounds, correct
shadows at terminal scales and independently rounded corners; these are
general rendering fixes for Blitz. The source patch is Apache-2.0.

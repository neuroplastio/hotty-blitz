---
title: "Fix box-shadow isolation, corner radii and blur size"
patch: 0016-Fix-box-shadow-isolation-corner-radii-and-blur-size.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-paint-0.3.0-beta.4/0007-Fix-box-shadow-isolation-corner-radii-and-blur-size.patch
---

Inset shadow compositing isolates destructive blending and scales its offset. Outset spread preserves each corner radius, with blurred nonuniform corners drawn in quadrants; the snapped inset ring remains for unblurred zero-offset shadows. Both shadow types use CSS Gaussian blur width at device scale. Hotty needs stable backgrounds and correct shadows at terminal scales; these are general Blitz rendering fixes. Adapted from kitesurf (Apache-2.0).

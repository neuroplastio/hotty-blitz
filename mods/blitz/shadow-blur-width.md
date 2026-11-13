---
title: Correct CSS box-shadow blur widths
patch: 0069-Correct-CSS-box-shadow-blur-widths.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-paint-0.3.0-beta.4/0007-Fix-box-shadow-isolation-corner-radii-and-blur-size.patch
---

Use the CSS Gaussian width at device scale for inset and outset shadows.
This is separated from the other box-shadow corrections because it changes
the appearance of existing shadows and remains parked pending a maintainer
decision. The correction would be appropriate upstream. Adapted from
kitesurf (Apache-2.0).

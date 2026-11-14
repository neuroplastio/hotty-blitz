---
title: "Preserve CSS shadow widths, corners and empty inset rings"
patch: 0061-Preserve-CSS-shadow-widths-corners-and-empty-inset-r.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-paint-0.3.0-beta.4/0007-Fix-box-shadow-isolation-corner-radii-and-blur-size.patch
---

Keep the CSS Gaussian blur width when painting through the newer vello backend. Skip an unblurred, unshifted inset shadow with nonpositive spread, whose coincident edges otherwise leave an antialiased seam. For a blurred outset shadow with differing corner radii, draw clipped quadrants using each corner’s radius instead of the backend average. Hotty needs the approved shadow widths, corner geometry and clean borders; these corrections suit upstream. The current upstream shadow API handles inset isolation and device-scaled offsets.

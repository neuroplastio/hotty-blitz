---
title: "Preserve CSS shadow widths and empty inset rings"
patch: 0061-Preserve-CSS-shadow-widths-and-empty-inset-rings.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-paint-0.3.0-beta.4/0007-Fix-box-shadow-isolation-corner-radii-and-blur-size.patch
---

Keep the CSS Gaussian blur width when painting through the newer vello backend. Skip an unblurred, unshifted inset shadow with nonpositive spread, whose coincident edges otherwise leave an antialiased seam. Hotty needs the approved shadow widths and clean borders; both corrections suit upstream. The other geometry corrections are handled by the current upstream shadow API.

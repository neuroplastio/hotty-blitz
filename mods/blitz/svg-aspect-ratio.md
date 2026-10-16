---
title: "Inline SVG follows its preserveAspectRatio"
patch: 0002-blitz-paint-inline-SVG-follows-its-preserveAspectRat.patch
upstream-fit: yes
---

**What it changes:** an inline `<svg>` was always painted as `object-fit:
contain`. A browser fits the viewBox to the element's box as
`preserveAspectRatio` says (SVG 2, 8.6): `none` stretches it, `slice`
covers the box, and the default fits inside. 14 lines in blitz-paint.

**Why we carry it:** charts drawn in a fixed viewBox and stretched to their
box came out narrow and centred.

**For upstream:** yes. It is what browsers do, and small.

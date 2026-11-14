---
title: Map CSS tab-size into text layout
patch: 0028-Map-CSS-tab-size-into-text-layout.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0051-Map-more-stylo-text-properties-onto-parley.patch
---

Pass the computed CSS tab size to Parley and invalidate text layout when
it changes. Preformatted text otherwise ignores the author's tab stops.
The mapping depends on Parley supporting tab sizes and could follow that
support upstream. Adapted from kitesurf (Apache-2.0).

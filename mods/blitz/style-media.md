---
title: Honour media attributes on style elements
patch: 0063-Honor-the-media-attribute-on-style-elements.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0014-Honor-the-media-attribute-on-style-elements.patch
---

Parse a style element's media attribute into the sheet's media list, so
print and viewport-specific styles only apply when their media matches.
Stylo then evaluates the list as the viewport changes. This HTML behavior
belongs upstream. Adapted from kitesurf (Apache-2.0).

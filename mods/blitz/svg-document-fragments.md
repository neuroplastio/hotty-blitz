---
title: Resolve inline SVG fragments across the document
patch: 0070-Resolve-inline-SVG-fragments-across-document.patch
upstream-fit: maybe
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0015-Resolve-use-href-id-against-the-whole-document-for-i.patch
---

Resolve an inline SVG's external same-document fragment references by adding
the referenced definitions before usvg parses the image. Follow chains with
cycle and size guards, and retain currentColor at each use site. Track each
consumer's targets; hotty deltas touching a target rebuild only dependent
SVGs, including when an ID binding changes. Render tests cover sprites,
chained references, color and updates; a parsed-tree identity test counts
zero rebuilds for an unrelated delta and one for a single dependent sprite.
The serialization approach is suitable for discussion upstream.

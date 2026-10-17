---
title: "An img shows the source that srcset, sizes and picture choose"
patch: 0008-blitz-dom-an-img-shows-the-source-srcset-sizes-and-p.patch
upstream-fit: yes
---

**What it changes:** Blitz loaded an `<img>`'s `src` and nothing else.
`BaseDocument::image_source` now selects a source as HTML does: the first
`<source>` of its `<picture>` whose `type` this build decodes and whose
`media` matches the device, else its own `srcset`, else its `src`. Of a
`srcset`, it takes the candidate with the smallest density at or above the
device's, else the densest; a `w` candidate's density is its width over
the size `sizes` gives it, with Stylo's `SourceSizeList`. Changing the
attributes loads the image again.

**Why we carry it:** hotty surfaces are drawn at the terminal's scale and
theme, and programs send images for each.

**For upstream:** yes. It is HTML's image source selection.

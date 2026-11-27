---
title: A bitmap glyph the atlas holds is not decoded again
patch: 0002-glifo-decode-a-bitmap-glyph-only-when-the-atlas-lack.patch
upstream-fit: yes
---

glifo decoded a bitmap glyph's PNG before it looked the glyph up in the
atlas. It needed the strike's width and height for the glyph's transform
and its cache key, so a glyph already in the atlas was decoded again every
time it was drawn. With this patch the size comes from the PNG's header
(IHDR), and the strike is decoded only when the atlas lacks the glyph. A
PNG whose header can't be read, or that fails to decode, falls back to the
outline as before.

A macOS sample of hotty-blitz, repainting text with colour emoji at a
display's rate, spent about 7% of its time in PNG decoding (inflate and
unfiltering) for glyphs the atlas already held.

For upstream: `yes`. It's a plain fix with no API change, and every glifo
user who draws colour emoji in text that redraws benefits. It builds on
"A colour emoji drawn smaller than its strike has smooth edges" only
through `Shrink::sized`. Without that patch, the change is the header read
and the decode moved after the lookup.

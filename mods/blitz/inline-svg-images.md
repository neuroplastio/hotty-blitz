---
title: "An inline SVG's image draws what the document fetches for it"
patch: 0009-blitz-dom-an-inline-SVG-s-image-draws-what-the-docum.patch
upstream-fit: yes
---

**What it changes:** usvg resolves an `<image>`'s href synchronously while
it parses, and Blitz parses an inline `<svg>` while it constructs boxes,
so nothing could be fetched for it. An SVG `<image>` is now fetched as an
`<img>` is, through the `NetProvider` as an `Image` request, when it joins
the document or its `href` changes. The bytes are kept by URL; when they
arrive, the outermost `<svg>` is built again, and the resolver hands usvg
those bytes. An SVG that is itself an image still loads only `data:`, as
in a browser.

**Why we carry it:** `cid:` resources in an SVG `href` (HOTTY SPEC §7.1),
and `img-src` reaching images in SVG (§7.2).

**For upstream:** yes. Inline SVG images then load as in a browser.

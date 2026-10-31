---
title: "A request says what it is for: image, style, font, iframe or document"
patch: 0006-blitz-traits-blitz-dom-a-request-carries-its-destina.patch
upstream-fit: yes
---

**What it changes:** a `NetProvider` got a URL and nothing about what it
was for. `Request` now has fetch's `destination`, and every request a
document makes sets it: `Image` for `<img>` and CSS images, `Style` for
stylesheets, their reload and `@import`, `Font` for `@font-face`,
`Iframe` for an iframe's document, and `Document` for a navigation.
`Request::get` leaves it `Empty`. The struct was `non_exhaustive`
already.

**Why we carry it:** hotty applies a fetch policy per kind of resource, as
CSP's `img-src`, `style-src` and `font-src` do (SPEC §7.2).

**For upstream:** yes. It is fetch's own field, and any embedder with a
content policy needs it.

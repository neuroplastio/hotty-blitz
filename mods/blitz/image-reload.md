---
title: "An image fetched again is shown again"
patch: 0105-blitz-dom-an-image-fetched-again-is-shown-again.patch
upstream-fit: yes
---

**What it changes:** `BaseDocument::reload_resource_by_href` now
reloads images too. The image at the href is dropped from the
document's image cache and fetched again. Every `<img>` showing it, and
every `background-image` or `mask-image` layer naming it, waits for it
as on a first load. An `<img>` is laid out again at the new image's
size when it arrives. An image still on its way is left to arrive.

**Why we carry it:** under SPEC §7.1, a resource sent again with other
bytes or another type replaces the old one, and every surface that
refers to it is drawn again. Before this, hotty-blitz drew those
surfaces again with the old image: the image cache answered for the
`cid:` URL, and only linked stylesheets and inline SVG images were
fetched again. An `<img>` also kept its old size, so a placement with
`f=1` never heard that the rows had changed (§5.2). hotty-blitz calls
this only for such a replace: a sending with the bytes and type the
resource has reaches no document, so an animated image plays on rather
than starting over.

**For upstream:** yes. `reload_resource_by_href` is how an embedder says
a URL's content changed, for example a dev server's hot reload. Images
are the other resource it can mean, and the change only reuses the
first-load path.

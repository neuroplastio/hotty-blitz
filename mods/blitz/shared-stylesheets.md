---
title: "Linked stylesheets parsed once for many documents"
patch: 0103-blitz-dom-an-embedder-can-share-a-linked-stylesheet-.patch
upstream-fit: maybe
---

**What it changes:** `DocumentConfig::shared_stylesheets`, a
`SharedStylesheets` the embedder implements, and
`parse_shared_stylesheet`. When a linked sheet's bytes arrive, the
document asks for its parsed contents first, naming the `<link>`, and if
it gets them wraps them in a stylesheet of its own, with its own lock and
`media`, rather than parsing. The embedder is not to give a document the
same contents for two links: Stylo tells sheets' media results apart by
their contents, and the second link's `media` changing would go unseen. Shared contents are parsed on a read-only lock, as Stylo's
user agent sheets are, so that any document's guard reads them. Their
`@import`s are resolved while they are parsed; one that cannot be is a
sheet each document parses for itself, since a pending import is written
in later through the sheet's lock. A shared sheet's imports get their
`@font-face` fonts fetched as a document's own do.

**Why we carry it:** a hotty program replaces a surface's document with a
new one (`a=doc`) on every tab switch, and each links the same `cid:`
sheets, which the host already holds. plx's kit sheet is 236 KB: parsing
it again was about 13 ms of a 27 ms switch. hotty-blitz now parses each
sending of a `cid:` sheet once per session, and keeps the parse while a
document holds it.

**For upstream:** maybe. Any embedder that shows many documents built
from the same sheets (tabs, a devtools panel, an app shell) pays the same
parse for each. The hook is small, but its contract, that contents be
on a read-only lock and complete, is one Blitz would want to own, for
example as a cache of its own keyed by URL and bytes.

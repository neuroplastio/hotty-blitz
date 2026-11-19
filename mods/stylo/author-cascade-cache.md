---
title: "Author cascade data shared between documents with the same sheets"
patch: 0014-Share-author-cascade-data-between-documents-with-the.patch
upstream-fit: maybe
---

**What it changes:** a document's author cascade data (selector maps,
invalidation maps, `@property`, `@keyframes`, layers) is now an `Arc`
looked up in a global cache, as the user agent origin's already is in
`UA_CASCADE_DATA_CACHE`. The key is the user agent cache's: the
contents of the sheets that apply, by identity, and their media query
results, plus the quirks mode. A document whose own data no other holds
rebuilds it in place, so an incremental rebuild stays incremental; one
it shares is rebuilt from a copy. The key holds the sheets' contents, so
a new sheet can never match a freed one's address, and an entry no
document uses is dropped at the next lookup.

**Why we carry it:** hotty-blitz builds a new document for every `a=doc`
(a tab switch in plx), and with mods/blitz 0103 each one links the same
parsed `cid:` sheets. Building the author data again for plx's 236 KB
kit sheet took about 2.5 ms per switch. Now only the first document of
a set of sheets builds it. A document with its own `<style>` has
contents no other has, and builds its own as before.

**For upstream:** maybe. Stylo already shares cascade data this way for
the user agent origin, and for shadow-DOM author sets within one
Stylist (`author_data_cache`). Sharing across documents needs their
sheets' contents to be the same `Arc`, which Servo would get from a
stylesheet cache of its own. A CSSOM change to a sheet shared between
documents would have to copy its contents first, as Gecko's do.

---
title: "A user agent stylesheet is parsed once"
patch: 0045-blitz-dom-a-user-agent-stylesheet-is-parsed-once.patch
upstream-fit: maybe
---

**What it changes:** `add_user_agent_stylesheet` parsed its CSS for every
document: Blitz's default is 24 KB, about 0.3 ms. Stylo then built the
user agent origin's cascade data again, because a sheet parsed anew is a
new key in its cache of that data (`UA_CASCADE_DATA_CACHE`). A user agent
sheet is now parsed once for the documents of a URL that add it, under a
read-only lock (`SharedRwLock::read_only`, which any document's guard
reads), and the process keeps the last eight. Every document after the
first shares the sheet, and Stylo's cache serves its cascade data. A sheet
with `@import` is still parsed per document, so its imports load through
that document's loader. Its relative URLs resolve as before, since the URL
is part of the key.

**Why we carry it:** a hotty host makes a document for every surface a
program opens, such as a palette or a dialog, and each paid for the parse
and the cascade data before its first frame. In `hotty replay` of a new
surface a frame (gov R-22), handling one went from 1.39 ms to 0.83.

**Platform:** neutral. It is Blitz and Stylo code only, the same on macOS.

**For upstream:** maybe. Any embedder that opens many documents gains,
and nothing a document sees changes. Upstream might rather have the
embedder parse the sheet once and hand it in through `DocumentConfig`,
or keep it on a shared context, than keep a cache in the process.

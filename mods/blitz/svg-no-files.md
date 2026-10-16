---
title: "An SVG's image loads a data: URL and no file"
patch: 0006-blitz-dom-an-SVG-s-image-loads-a-data-URL-and-no-fil.patch
upstream-fit: yes
---

**What it changes:** Blitz parses every SVG (an `<img>`, a CSS image and an
inline `<svg>` alike) with usvg's default options, whose image resolver
reads any `<image href>` that isn't a `data:` URL as a path on the disk. A
page could draw any image the user's files hold, without asking the
`NetProvider`. The resolver now loads `data:` URLs, and (with a later
patch) what the document fetched for an inline SVG's `<image>`, and
nothing else. A test in blitz-dom checks it.

**Why we carry it:** HOTTY documents come from programs, and a surface
must never read the user's files (SPEC §12).

**For upstream:** yes. It closes a file read that browsers never allow.

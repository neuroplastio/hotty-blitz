---
title: "The system is asked for a fallback once"
patch: 0003-fontique-ask-the-system-for-a-fallback-once-per-coll.patch
upstream-fit: yes
---

**What it changes:** a collection's clones share the system's fonts, but
the fallback family the system gave for a key (a script and a locale)
was kept only in the clone that asked. Each new clone asked again, and
with fontconfig that is `FcFontMatch`, a match over every installed font.
Parley asks for two keys on every query, the text's own and Han's. The
answer is now kept beside the system's fonts in `System`, for every
clone, and so is a key the system has no fallback for.

**Why we carry it:** hotty-blitz gives each surface a clone of one font
context, so every new surface asked fontconfig again. In `hotty replay`
of a new surface a frame (gov R-22), handling one went from 0.83 ms to
0.37.

**Platform:** neutral. The cache sits in the collection, above the
backends, so it holds for CoreText and DirectWrite as for fontconfig.
It was measured on fontconfig only: what it saves elsewhere is what that
backend's lookup costs.

**For upstream:** yes. The family lookup next to it is already cached in
each backend (`family_map`), and nothing a caller sees changes, as long
as the system's fonts don't change under a running process, which
fontique doesn't watch for anyway.

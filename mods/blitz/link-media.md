---
title: Scope linked stylesheets to their media
patch: 0064-Scope-linked-stylesheets-to-their-media-attribute.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0035-Scope-linked-stylesheets-to-their-media-attribute.patch
---

Apply a link's media attribute to its fetched stylesheet, and recascade
when the attribute changes. The current attribute is read when the fetch
finishes, so changes while it is in flight are not lost. The same mutation
handling applies to style elements. This HTML behavior belongs upstream.
Adapted from kitesurf (Apache-2.0).

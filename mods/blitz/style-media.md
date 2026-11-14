---
title: "Honor media attributes on style and link elements"
patch: 0037-Honor-media-attributes-on-style-and-link-elements.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0014-Honor-the-media-attribute-on-style-elements.patch; kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0035-Scope-linked-stylesheets-to-their-media-attribute.patch
---

Apply media attributes on inline and linked stylesheets, including attribute changes and changes made while a link is fetching. Stylo evaluates the media list against the current device. Hotty needs styles scoped to the viewport and theme; this HTML behaviour belongs upstream. Adapted from kitesurf (Apache-2.0).

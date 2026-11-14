---
title: Clear surviving layout-parent links on removal
patch: 0062-Clear-surviving-layout-parent-links.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0062-Clear-layout_parent-on-nodes-that-outlive-their-layo.patch
---

When an anonymous or other layout parent is removed, clear references to it
on surviving DOM and layout children. Otherwise later geometry walks can
follow a freed node ID and panic. A regression covers both kinds of child;
this is a general robustness fix suitable for Blitz upstream.

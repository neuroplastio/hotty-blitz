---
title: "A box whose layout changed gets its overflow and transform again"
patch: 0004-blitz-dom-a-box-whose-layout-changed-gets-its-overfl.patch
upstream-fit: yes
---

**What it changes:** Blitz computed a node's transform and scrollable
overflow again only when its own style damage asked for it. Damage flows up
the tree, so a box resized only because an ancestor changed kept its old
overflow, and paint, which culls a box by its overflow, could drop it on
every later paint. Every node whose final layout changed is now recorded
(`set_final_layout`), with the boxes that reach it in `resolve_transforms`,
and those are computed again too.

**Why we carry it:** a delta to a custom property that set a ring's height
and `translateY` left its stretched child culled off the surface.

**For upstream:** yes. It is a bug in incremental updates that any
embedder changing a document can hit.

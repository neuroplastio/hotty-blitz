---
title: Inert subtrees and hidden contents
patch: 0107-blitz-dom-skip-hidden-contents-and-hit-through-inert.patch
upstream-fit: yes
---

Skip the children of a box with content-visibility hidden when constructing
layout and reconstruct it when that property changes. Skip inert subtrees
during hit testing and keep both out of sequential focus traversal. hotty uses the same queries for HTML focus
fixup. These are browser behaviors that also apply to other Blitz hosts.

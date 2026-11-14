---
title: Propagate text decorations into in-flow block children
patch: 0072-Propagate-text-decorations-into-in-flow-block-children.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-paint-0.3.0-beta.4/0023-Propagate-text-decorations-into-in-flow-block-children.patch
---

A decorating ancestor continues across in-flow block formatting roots, while atomic inline, float and out-of-flow boundaries stop it. Ancestor paint layers supply their own opacity. A pixel test checks that an underline reaches a block child but does not cross an inline-block boundary.

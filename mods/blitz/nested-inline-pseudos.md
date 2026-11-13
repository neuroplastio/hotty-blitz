---
title: Pseudo-elements on nested inlines appear
patch: 0007-Flush-nested-inline-pseudo-elements-child-display-no.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0013-Flush-nested-inline-pseudo-elements-child-display-no.patch
---

The inline construction walk now decides whether to descend from each
child's display rather than its parent's. This creates `::before` and
`::after` on inlines nested below another inline, so hotty documents can
use common icon and generated-text patterns. The change fixes a general
Blitz rendering bug and should be useful upstream. The source patch is
Apache-2.0.

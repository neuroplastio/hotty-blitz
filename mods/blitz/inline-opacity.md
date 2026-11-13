---
title: Apply opacity to inline text and backgrounds
patch: 0062-Apply-inline-box-opacity-to-text-and-span-decoration.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-paint-0.3.0-beta.4/0018-Apply-inline-box-opacity-to-text-and-span-decoration.patch
---

Apply the product of inline ancestor opacities to glyphs, text decorations
and backgrounds, so an inline span can fade. The inline root already has a
paint layer and is excluded. Like the kitesurf patch, per-run alpha is an
approximation to group opacity where painted runs overlap; an upstream
implementation could instead composite the whole inline box. Adapted from
kitesurf (Apache-2.0).

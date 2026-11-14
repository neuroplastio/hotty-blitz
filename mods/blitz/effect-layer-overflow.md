---
title: Size effect layers to include overflow
patch: 0074-Size-effect-layers-to-include-overflow.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-paint-0.3.0-beta.4/0008-Size-the-effect-layer-to-the-painted-subtree.patch
---

Union the node's painted overflow into its effect-layer bounds before expanding for a filter. The current overflow pass already accounts for positioned descendants of the containing block. Pixel tests cover an overflowing positioned child under a no-op filter and an in-flow child under opacity. An ignored test records the separate paint-tree bug where an opacity ancestor does not contain a positioned descendant whose containing block is outside it; that belongs to leg 5. The shipping CPU renderer does not currently apply filters when multithreading is enabled, so this test cannot verify the blur expansion itself.

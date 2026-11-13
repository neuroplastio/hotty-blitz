---
title: Recognize tabs in the text engine
patch: 0005-Expand-horizontal-tabs-to-CSS-tab-size-stops.patch
upstream-fit: yes
source: kitesurf scripts/patches/parley_engine-0.12.0/0002-Expand-horizontal-tabs-to-CSS-tab-size-stops.patch
---

Classify horizontal tabs separately from other whitespace so layout can
expand them at tab stops. This supports CSS tab-size and should be useful
to other Parley consumers. Adapted from kitesurf (Apache-2.0).

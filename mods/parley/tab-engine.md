---
title: "Expand horizontal tabs to CSS tab stops"
patch: 0005-Expand-horizontal-tabs-to-CSS-tab-stops.patch
upstream-fit: yes
source: kitesurf scripts/patches/parley_engine-0.12.0/0002-Expand-horizontal-tabs-to-CSS-tab-size-stops.patch; kitesurf scripts/patches/parley-0.12.0/0006-Expand-horizontal-tabs-to-CSS-tab-size-stops.patch
---

Recognize horizontal tabs in the text engine and expand preserved tabs at CSS tab-size stops during layout. Cluster and glyph advances keep painting, selection and hit testing consistent. Hotty needs preformatted text with correct tabs; this general layout behaviour is useful to Parley. Adapted from kitesurf (Apache-2.0).

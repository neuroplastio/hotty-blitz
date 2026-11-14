---
title: Paint range inputs using their value and accent
patch: 0077-Paint-range-inputs-using-their-value-and-accent.patch
upstream-fit: maybe
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0012-Support-range-input-values-UA-style-and-thumb-colour.patch; scripts/patches/blitz-paint-0.3.0-beta.4/0003-Paint-range-inputs-and-select-chevrons-in-accent-col.patch
---

Range inputs keep a finite clamped value and paint a track and thumb using the computed accent; checkbox and radio controls also honor that accent. Host value/min/max deltas update the slider. Legacy thumb background rules are lifted to accent-color in their original CSS group and order, including linked and imported UTF-8 sheets. The existing select patch already supplies the closed-control arrow. Pixel tests cover value changes, removal and media-scoped legacy colors; parser tests exercise the rewrite.

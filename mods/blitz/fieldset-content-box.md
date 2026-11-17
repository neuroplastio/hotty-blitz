---
title: A fieldset lays its rendered legend out outside its content
patch: 0095-blitz-dom-a-fieldset-lays-its-rendered-legend-out-ou.patch
upstream-fit: yes
---

HTML renders a fieldset as its rendered legend (its first `legend` child,
if that is neither floated nor absolutely positioned) and an anonymous
fieldset content box holding everything else. The content box takes the
fieldset's `display`, its flex and grid container properties and its
padding. Blitz laid the legend out as one more child. In a flex fieldset it
was a flex item: stretched to the full width, with the `gap` after it. In
any fieldset it sat inside the padding instead of above it.

Construction now gives a flow, flex or grid fieldset with a rendered legend
two layout children: the legend, and an anonymous box styled from the
fieldset (`fieldset_content_style`) that holds the rest, pseudo-elements
included. A node flag makes the fieldset lay the two out in a flow root of
its own, and the Taffy style wrapper gives it no padding, since the content
box has it. When the fieldset is restyled, its content box gets that style
again, not a plain anonymous block's, which would only rebuild the
fieldset. The UA sheet makes the rendered legend as wide as its content.
The legend still sits inside the top border rather than over it, and a
content box does not yet grow to fill a fieldset of definite height.

We carry it for hotty-surf's kit: Basecoat's FieldSet, the commonest form
layout, is a flex column with a gap and a legend, and in Blitz its legend
was a stretched bar with a gap under it. Upstream should want it: it is the
HTML standard's rendering, and Chromium and Firefox both build the same
anonymous box.

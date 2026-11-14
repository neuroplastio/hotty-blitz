---
title: Propagate eligible body overflow to the viewport
patch: 0067-Propagate-body-overflow-to-the-viewport.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0065-report-when-body-s-overflow-belongs-to-the-viewport.patch; scripts/patches/blitz-paint-0.3.0-beta.4/0022-Don't-clip-a-body-whose-overflow-went-to-the-viewpor.patch
---

When the root has visible overflow, its first displayed body passes overflow
to the viewport instead of clipping descendants at the body's own edge.
Preserve independent paint containment and clipping where the root itself
clips. A host render test covers a popup below a short body and both clipping
counterexamples. This general CSS Overflow fix suits upstream.

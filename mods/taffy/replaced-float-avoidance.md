---
title: Keep block replaced elements clear of floats
patch: 0002-Keep-block-replaced-elements-clear-of-floats.patch
upstream-fit: yes
source: kitesurf scripts/patches/taffy-0.15.0/0002-Fix-float-placement-and-sizing-in-block-layout.patch
---

Replaced block items use the independent float-avoidance path, so a canvas beside a float does not overlap it. Upstream f6b87ac4 already accounts for bottom padding and border beneath floats. The original port's collapsed-first-child position adjustment conflicts with the newer float-clearance logic: a cleared sibling then overlaps the float. A regression checks both that clearance and replaced-item avoidance.

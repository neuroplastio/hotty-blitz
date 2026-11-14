---
title: Report the last baseline of block containers
patch: 0001-Report-a-block-container-last-baseline.patch
upstream-fit: yes
source: kitesurf scripts/patches/taffy-0.15.0/0007-Report-a-block-container-s-last-baseline.patch
---

Block containers expose the last in-flow child's baseline, allowing an inline block with nested block content to align like one whose content is direct. Preserve the pinned Taffy's handling of relative positioning and scroll-container baselines. A host test covers both nested content and a relative offset.

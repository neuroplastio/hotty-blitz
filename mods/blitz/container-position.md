---
title: Keep size containers from claiming positioned boxes
patch: 0066-Stop-size-containers-from-claiming-out-of-flow-boxes.patch
upstream-fit: yes
source: kitesurf scripts/patches/stylo_taffy-0.3.0-beta.4/0002-Stop-size-containers-from-claiming-out-of-flow-boxes.patch
---

Size containment does not itself establish the containing block for
positioned descendants. Remove the extra claim so those boxes use their
proper ancestor. This CSS behavior belongs upstream. From kitesurf
(Apache-2.0).

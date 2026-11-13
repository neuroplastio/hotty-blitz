---
title: Settle container queries after layout
patch: 0067-Restyle-container-queries-against-laid-out-container.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0048-Restyle-container-queries-against-laid-out-container.patch
---

Restyle containers whose content-box sizes changed and repeat layout,
up to eight passes, so nested queries see the sizes produced by their
ancestors. A cap prevents oscillation from blocking a resolve. This
integration belongs upstream with container query support. Adapted from
kitesurf (Apache-2.0) to the current resolve pipeline.

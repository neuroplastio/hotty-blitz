---
title: Skip unnecessary paint work
patch: 0088-Skip-unnecessary-paint-work.patch
upstream-fit: maybe
source: kitesurf scripts/patches/blitz-paint-0.3.0-beta.4/0011-paint-skip-work-the-common-element-does-not-need.patch
---

Avoid constructing unused clip paths or resolving colors for zero-width borders. Current upstream already skips empty image layers and lazily constructs the fallback style. hotty-perf measured 04-table 12.8% faster and 06-effects, 01-card, e1p and 05-forms 5–6% faster, with no measured regression and identical pixels; its A/A control called nothing at load 1.6–4.3.

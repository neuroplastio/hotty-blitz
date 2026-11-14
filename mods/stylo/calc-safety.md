---
title: Keep computed calc trees correct and panic-free
patch: 0011-Keep-computed-calc-trees-correct-and-panic-free.patch
upstream-fit: yes
source: kitesurf scripts/patches/stylo-0.22.0/0010-Keep-computed-calc-trees-correct-and-panic-free.patch
---

Handle empty computed sums and invalid or numeric length-percentage results
without panicking. Preserve unit multipliers around non-distributive
functions and avoid reducing incompatible NaN branches. A regression checks
an empty computed sum. This general calc correctness fix is suitable for
Stylo upstream.

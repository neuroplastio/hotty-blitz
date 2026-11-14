---
title: Convert color-matrix filter chains
patch: 0002-Convert-color-matrix-filter-chains.patch
upstream-fit: yes
---

Pass color matrices to Vello CPU instead of dropping them. Linear matrix-only chains are preserved: adjacent matrices compose when their intermediate output cannot require clamping, otherwise they remain separate passes. Tests cover order and the separate-pass case. Other graphs retain upstream's first-primitive behavior.

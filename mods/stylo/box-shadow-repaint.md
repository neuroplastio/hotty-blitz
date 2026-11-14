---
title: Box-shadow changes repaint without rebuilding layout
patch: 0008-Damage-box-shadow-as-repaint-instead-of-rebuilding-t.patch
upstream-fit: yes
source: kitesurf scripts/patches/stylo-0.22.0/0009-Damage-box-shadow-as-repaint-instead-of-rebuilding-t.patch
---

A box-shadow change now produces repaint damage rather than rebuilding the
box. The shadow changes ink without changing layout, so hotty animations and
updates can avoid unnecessary layout work. This classification is generally
useful to Stylo embedders. The source patch is Apache-2.0.

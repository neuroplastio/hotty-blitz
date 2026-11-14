---
title: Resolve inline box heights and relative insets
patch: 0066-Resolve-inline-box-heights-and-relative-insets.patch
upstream-fit: yes
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0049-Fix-intrinsic-heights-and-relative-insets-of-inline-.patch
---

Resolve intrinsic and stretch height keywords when measuring atomic inline
boxes. Resolve relative inline block percentages against a definite containing
height, and apply relative insets to floats after their flow positions are
chosen. Tests cover stretch height and a shifted relative float. This general
layout correction suits upstream.

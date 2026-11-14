---
title: Enable accent-color in the Servo build
patch: 0012-Enable-accent-color-in-the-Servo-build.patch
upstream-fit: yes
source: kitesurf scripts/patches/stylo-0.22.0/0002-Enable-gecko-gated-properties-and-values-in-the-serv.patch
---

Expose the already implemented accent-color property in Servo builds so the Blitz form-control painter can use an authored accent. The range pixel tests exercise the property as well as the legacy thumb-color compatibility path.

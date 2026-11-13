---
title: Enable container query syntax in Servo
patch: 0004-Enable-container-query-syntax-in-Servo.patch
upstream-fit: yes
source: kitesurf scripts/patches/stylo-0.22.0/0005-Enable-container-queries-cq-units-and-anchor-parsing.patch
---

Enable container properties, the shorthand, @container and container
units in Servo. The layout integration supplies actual container sizes.
Upstream could enable the same syntax once its embedding supplies size
queries. Adapted from kitesurf (Apache-2.0) for Stylo 0.23; unrelated
anchor-position parsing is not included.

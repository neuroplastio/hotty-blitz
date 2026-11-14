---
title: Enable legacy WebKit boxes and line clamp
patch: 0013-Enable-legacy-WebKit-boxes-and-line-clamp.patch
upstream-fit: yes
source: kitesurf scripts/patches/stylo-0.22.0/0002-Enable-gecko-gated-properties-and-values-in-the-serv.patch
---

Enable the existing WebKit box, orient and legacy line-clamp style machinery for Servo and flag the flow-root rewrite for Blitz layout. The generated XUL style pointer increases 64-bit Servo `ComputedValues` from 232 to 240 bytes. The retained `size_of_test!(ComputedValues, 240)` assertion fails the build if it grows again. Gov accepted this increase for the port.

---
title: Finished animations keep their forwards fill
patch: 0007-Retain-forwards-filling-animations-after-they-finish.patch
upstream-fit: yes
source: kitesurf scripts/patches/stylo-0.22.0/0008-Retain-forwards-filling-animations-and-add-script-an.patch
---

Finished animations with a forwards fill stay in the animation set so their
final values survive later restyles. Canceling one marks the set dirty so
its contribution is removed. Finished fills do not request more ticks.
Hotty needs stable animated surfaces and correct damage after cancellation;
the same CSS behavior is useful to Stylo embedders. The source patch is
Apache-2.0; only its CSS animation portion is carried.

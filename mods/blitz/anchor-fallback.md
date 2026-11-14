---
title: Handle unresolved anchor values during layout conversion
patch: 0063-Handle-unresolved-anchors-in-conversion.patch
upstream-fit: yes
source: kitesurf scripts/patches/stylo_taffy-0.3.0-beta.4/0001-Handle-unresolved-anchor-values-without-panicking.patch
---

Stylo may represent anchor-dependent values before Taffy can resolve them.
Use their explicit fallback, or the property's initial layout value, instead
of panicking. Tests cover an explicit size fallback and the no-fallback
case. This is suitable upstream until anchor positioning is supported.

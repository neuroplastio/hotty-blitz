---
title: "prefers-reduced-motion in Servo mode"
patch: 0004-Servo-add-the-prefers-reduced-motion-media-feature.patch
upstream-fit: yes
---

**What it changes:** Stylo in Servo mode had no `prefers-reduced-motion`
media feature, so `@media (prefers-reduced-motion: reduce)` never matched
(nor did `no-preference`: the feature was unknown). It now parses and evaluates, `no-preference` or `reduce` (or the bare feature, true
for `reduce`), from `MediaData::prefers_reduced_motion`, a value on the
Device that the embedder sets, as it sets `prefers_color_scheme`
(`Device::set_prefers_reduced_motion`, and a getter). The keyword enum and
the evaluator are Gecko's shape, without its binding. It adds an atom.

**Why we carry it:** a person who asked their system for less motion should
get it from a hotty surface, whose stylesheets (hotty-surf's kit) switch
transitions off under that media query (hotty V8-01, SPEC 8).

**For upstream:** yes. It is a standard Media Queries 5 feature that Servo
and Blitz lack; the change is small and follows the colour scheme's route.
It adds a public field to `MediaData`, which an embedder that builds one
must fill, so upstream may want it behind `Default`.

**Platform:** neutral. The embedder reads the platform's setting.

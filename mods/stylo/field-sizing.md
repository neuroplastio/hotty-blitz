---
title: "field-sizing in Servo mode, behind a pref"
patch: 0002-Enable-field-sizing-behind-a-pref.patch
upstream-fit: yes
---

**What it changes:** `field-sizing` was Gecko's only, so Stylo in Servo
mode dropped it at parse. It now parses, computes and serializes in Servo
mode behind the static pref `layout.css.field-sizing.enabled`, off by
default, as `scroll-behavior` and the scroll-snap properties are. The
property keeps its layout damage.

**Why we carry it:** a multi-line field that grows with its text from a
minimum to a maximum number of rows is `field-sizing: content` with
`min-height` and `max-height`. Blitz reads the computed value to size a
textarea (mods/blitz/field-sizing.md) and sets the pref. Without this patch
the declaration never reaches Blitz. hotty-blitz's `tests/field_sizing.rs`
checks the result.

**For upstream:** yes. It is how Stylo has enabled other properties a
Servo-mode embedder implements first: one pref, off by default, so Servo
is unchanged. servo/stylo's `main` has the same property entry as `base`,
so the patch applies there as it is.

**Platform:** neutral. It is Stylo code only, the same on macOS.

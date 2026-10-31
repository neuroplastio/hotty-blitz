---
title: "Scroll Snap's properties in Servo mode, behind a pref"
patch: 0001-Enable-CSS-Scroll-Snap-s-properties-behind-a-pref.patch
upstream-fit: yes
---

**What it changes:** `scroll-snap-type`, `scroll-snap-align`,
`scroll-snap-stop`, the eight `scroll-padding` longhands and their three
shorthands were Gecko's only, so Stylo in Servo mode dropped them at
parse. They now parse, compute and serialize in Servo mode behind a new
static pref, `layout.css.scroll-snap.enabled`, off by default, as
`scroll-behavior` and `overscroll-behavior` are. `ScrollSnapType`'s and
`ScrollSnapAlign`'s fields are public, so that an embedder can read a
computed value; Gecko reads them through cbindgen. The longhands keep
Servo mode's default restyle damage, as `scroll-behavior` does.

**Why we carry it:** Blitz snaps scrolls with these properties
(mods/blitz/scroll-snap.md) and sets the pref. Without this patch a
document's `scroll-snap-type` never reaches Blitz. hotty-blitz's
`tests/snap.rs` checks what Stylo parses, computes and serializes.

**For upstream:** yes. It is how Stylo has enabled other properties a
Servo-mode embedder implements first: one pref, off by default, so Servo
is unchanged. Upstream might rather add getters than make the fields
public, and might give the longhands less damage than a box rebuild, as
none of them changes layout. servo/stylo's `main` has the same tree as
`base`, so the patch applies there as it is.

**Platform:** neutral. It is Stylo code only, the same on macOS.

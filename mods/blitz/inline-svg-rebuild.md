---
title: "An inline svg is built again when its source changes"
patch: 0028-blitz-dom-an-inline-svg-is-built-again-when-its-sour.patch
upstream-fit: maybe
---

**What it changes:** an inline `<svg>` is drawn from one usvg parse of its
markup, made while boxes are constructed, with `currentColor` resolved to
each element's colour then. A colour change later is only style damage, so
an icon drawn with `fill="currentColor"` kept its first colour through a
parent's class, a theme, `:hover` and `:focus`. A hash of the source each
inline svg was built from is now kept, and when an svg or anything under
it is damaged and its source differs, that svg alone is constructed again.

**Why we carry it:** the hotty UI kit draws its icons in `currentColor`.

**For upstream:** maybe. The bug is real, but Blitz has since rebuilt
inline SVG when its styles change in its own way, after our base, which
may make this patch unnecessary once the base moves.

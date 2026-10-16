---
title: "Outlines take outline-offset and every border style"
patch: 0004-blitz-paint-outlines-take-outline-offset-and-every-b.patch
upstream-fit: yes
---

**What it changes:** an outline was drawn solid, just outside the border
box: `outline-offset` was ignored, and dashed, dotted, double, groove,
ridge, inset and outset outlines came out solid. The outline is now the
border of a box of its own, offset from the border box (inside it when the
offset is negative), drawn with the border's edge code, so it takes every
border style. It is painted after the element's content, as CSS 2.1
Appendix E does, so the element's background doesn't cover one inside it.
Dotted outlines have round dots, as Blitz's dotted borders do.

**Why we carry it:** a surface's root fills the surface, so an outline on
it has to come inside with a negative offset, and was drawn off the
surface.

**For upstream:** yes. It is CSS as browsers draw it.

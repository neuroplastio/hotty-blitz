---
title: Keep the existing box-shadow blur width
patch: 0059-Keep-the-existing-box-shadow-blur-width.patch
upstream-fit: no
---

The box-shadow geometry port also changed Gaussian blur widths. This patch
keeps the previous widths while retaining its other corrections, so hotty
surfaces keep the shadow appearance their styles were tuned for. The
separate CSS-correct width change is pending a maintainer decision; keeping
the older width serves this integration rather than upstream behavior.

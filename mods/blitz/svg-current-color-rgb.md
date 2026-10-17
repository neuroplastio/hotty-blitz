---
title: "An inline svg's currentColor is written as rgb()"
patch: 0029-blitz-dom-an-inline-svg-s-currentColor-is-written-as.patch
upstream-fit: yes
---

**What it changes:** `outer_html` wrote an inline svg's `currentColor` with
`to_css_string`, so a colour that computes to anything but legacy sRGB
(`color-mix()`, oklab, display-p3) came out as `color(srgb …)`, which usvg
doesn't parse, and the icon drew black. It is now written as `rgb()` or
`rgba()` (`into_srgb_legacy`).

**Why we carry it:** every icon in the hotty UI kit's muted text, a
`color-mix()`, drew black.

**For upstream:** yes. It is small, and any page with modern colours hits
it.

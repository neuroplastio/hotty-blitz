---
title: "Form controls follow the used color scheme"
patch: 0011-blitz-dom-blitz-paint-form-controls-follow-the-used-.patch
upstream-fit: yes
---

**What it changes:** the UA stylesheet made fields white and buttons light
grey, and checkboxes and radio buttons were painted white, so a page with
`color-scheme: dark` showed light controls. Fields now take `Field` and
`FieldText`, and buttons `ButtonFace` and `ButtonText`, system colours
that Stylo already resolves in the element's used colour scheme, with
Chromium's borders in each scheme (`light-dark()`). Checkboxes and radio
buttons are drawn in the used scheme (`BaseDocument::used_color_scheme`),
with Chromium's colours for `accent-color: auto` and its 13px geometry. A
document's own colours still win.

**Why we carry it:** surfaces on a dark terminal are dark (hotty SPEC
§8).

**For upstream:** yes. Browsers draw controls this way; the shades are
Stylo's system colours, a little off Chromium's.

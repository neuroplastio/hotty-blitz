---
title: A check box with appearance none is an ordinary box
patch: 0093-blitz-dom-blitz-paint-a-check-box-with-appearance-no.patch
upstream-fit: yes
---

Blitz's UA sheet never gave the form controls `appearance: auto`, and
nothing read `appearance`. A check box or radio button was always laid out
as a square of its smaller side and drawn as the native widget, and its
`::before` and `::after` were never built.

The patch gives `input`, `button`, `select`, `textarea`, `meter` and
`progress` `appearance: auto`, as HTML's rendering section does, and adds
`Node::has_native_appearance`. When a page sets `appearance: none` on a
check box or radio button, construction goes on past the checkbox state to
build its children by its `display`, its pseudo-elements among them; layout
skips the square; paint draws no native widget; and it no longer gets the
compressible form control flag, since it is now an ordinary box. Chromium
and Firefox both draw such an input from its styles alone. Range inputs keep
their native slider: Blitz has no `::-webkit-slider-thumb` to draw one
styled by the page.

We carry it for hotty-surf's kit. Basecoat's switch is an
`input[type=checkbox][role=switch]` with `appearance: none` and a
`::before` thumb, and its check box draws the tick with an `::after` mask:
in Blitz the switch read as a check box, so the user could not tell it was a
switch, and the native box covered the kit's own.

Upstream should want it: `appearance: none` is how pages restyle these
controls, and the UA sheet line is the HTML standard's.

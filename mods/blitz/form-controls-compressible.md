---
title: Text fields, selects and textareas are compressible replaced elements
patch: 0091-blitz-dom-text-fields-selects-and-textareas-are-comp.patch
upstream-fit: yes
---

Adds a style flag, `IS_FORM_CONTROL`, set on every `input` but the button
types, `image` and `hidden`, and on every `select` and `textarea`. Taffy's
`is_compressible_replaced` now answers true for them, as its own docs ask
("a form control with an intrinsic size"). Before, Blitz answered only for
replaced elements (`img`, `svg`, `canvas`, ...).

Taffy then lays such a control out as browsers do. As a block, it keeps its
own width instead of filling the line: Chromium gives a `display: block`
text field of `size=20` its 185 px, not its 400 px container. In a
shrink-to-fit container a percentage width caps its min-content
contribution at zero. In a flex row, with mods/taffy 0006, a `width: 100%`
field shrinks beside its siblings.

We carry it for basecoat's command box: a search field with `width: 100%`
beside a 16 px icon, in a 200 px row. Chromium gives it 176 px. Blitz gave
it 200, so it ran past the row, and the storybook's Popover + command
showed its content shifted sideways.

Upstream should want it: it is the flag Taffy's trait was made for, and
Blitz's text fields already size like replaced elements (their intrinsic
width comes from `size`). Buttons stay ordinary boxes; Chromium does not
shrink a `width: 100%` button below its label.

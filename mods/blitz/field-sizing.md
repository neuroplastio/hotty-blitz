---
title: "A textarea with field-sizing: content is as tall as its text"
patch: 0054-blitz-dom-a-textarea-with-field-sizing-content-is-as.patch
upstream-fit: maybe
---

**What it changes:** a textarea was always `rows` lines tall. With
`field-sizing: content` its layout runs twice: once for the width it gets,
then for the height of its text wrapped at that width, which
`min-height` and `max-height` bound as for any box. Past the most the text
scrolls inside the box, following the caret, as it already did for a
textarea with fixed rows. An edit that changes the text's height lays the
field out again, for which a text input that sizes to its text reports its
line metrics as a single-line one does. The width stays `cols`: only the
block size follows the text. Blitz sets `layout.css.field-sizing.enabled`,
which Stylo's Servo mode needs to parse the property (mods/stylo).

**Why we carry it:** hotty-surf's multi-row field is a textarea that grows
from a minimum to a maximum number of rows and then scrolls, and its
height must reach the document's layout, so that `fit` and the box
under it follow.

**For upstream:** maybe. It is the CSS UI 4 behaviour for the block axis
of a textarea. Upstream would want the inline axis too (an auto width that
follows the text), and an `<input>` that sizes to its value.

**Platform:** neutral.

---
title: "A textarea sized to its text does not follow its caret when laid out"
patch: 0055-blitz-dom-a-textarea-sized-to-its-text-does-not-foll.patch
upstream-fit: no
---

**What it changes:** patch 0054 clamped a content-sized textarea's scroll
offset at every layout with the call that also scrolls the caret into
view, so an unfocused textarea whose caret sat at the end of a long text
showed its end. Layout now only takes the offset back to the most the new
height allows; edits and focus still follow the caret as before.

**Why we carry it:** a browser leaves an unfocused textarea at the top,
which hotty-surf's gate compares against (field-rows, 0.7.2).

**For upstream:** none by itself: it corrects 0054, which would be offered
as one change.

**Platform:** neutral.

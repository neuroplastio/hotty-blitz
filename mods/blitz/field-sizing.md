---
title: "Size a textarea to its content"
patch: 0059-Size-a-textarea-to-its-content.patch
upstream-fit: maybe
---

A textarea with field-sizing: content grows to its wrapped text height between min-height and max-height. Layout clamps its scroll offset without moving an unfocused caret into view; editing and focus still follow the caret. Hotty's growing multi-row fields need this. Upstream would also want content-based width and input sizing.

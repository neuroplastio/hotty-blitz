---
title: "A text input's baseline is its text's"
patch: 0034-blitz-dom-a-text-input-s-baseline-is-its-text-s.patch
upstream-fit: yes
---

**What it changes:** a single-line text input had no baseline, so a flex
row with `align-items: baseline` and a line of text both put its bottom
edge on the baseline, and its text sat above the label beside it (6px at
20px, 22px for a tall padded input). Its first and last baselines are now
its line's, placed where blitz-paint draws the text, centred in the
content box. `TextInputData::line_metrics` gives the line's height and
baseline. A fallback font (CJK, say) can make the line taller, as in a
browser, so an edit that changes the line's metrics, by key, IME, the
`value` attribute or the embedder's setters, marks the input for
relayout; other edits cost no layout.

**Why we carry it:** pages put inputs beside labels and chips in
baseline-aligned rows and in running text.

**For upstream:** yes. It is browser behaviour in blitz-dom's own leaf
layout, and upstream has the same bug. A textarea (no baseline, no
`overflow: auto`) and a select (taffy's grid baselines) still differ
from Chromium. The layout repeats paint's centring; upstream may prefer
one helper.

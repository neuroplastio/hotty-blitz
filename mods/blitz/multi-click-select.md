---
title: "A double click selects a word, and a triple click its block"
patch: 0029-blitz-dom-a-double-click-selects-a-word-and-a-triple.patch
upstream-fit: yes
---

**What it changes:** in a page's text, as a browser does: a double click
selects the word under the pointer (`Selection::word_from_point`), and a
triple click the inline root's whole text. A text input already did both.

**Why we carry it:** hotty documents are read and copied from like pages
(SPEC §9).

**For upstream:** yes. It is what browsers do.

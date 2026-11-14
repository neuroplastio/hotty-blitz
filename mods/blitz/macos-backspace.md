---
title: "Backspace edits a text input on macOS too"
patch: 0018-blitz-dom-Backspace-edits-a-text-input-on-macOS-too.patch
upstream-fit: maybe
---

**What it changes:** the text input's Backspace arm was compiled out on
macOS, where Cocoa's text system turns the key into a `deleteBackward:`
keybinding that does the deleting. The arm now applies on every platform.
It is the only key the text input left to Cocoa: Delete, the arrows, Home
and End take the same path everywhere.

**Why we carry it:** a terminal has no Cocoa text system and hotty-blitz
sends no keybindings, so on macOS Backspace did nothing in a field.

**For upstream:** maybe. An embedder without Cocoa needs it, but
upstream's own macOS shell would then delete twice unless it is made
conditional on the embedder.

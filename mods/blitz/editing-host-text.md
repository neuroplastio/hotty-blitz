---
title: Multiline editors for HTML editing hosts
patch: 0109-Give-HTML-editing-hosts-a-multiline-editor.patch
upstream-fit: maybe
---
Recognize editing hosts using inherited HTML editability. Lay out their plain text in the multiline editor so hotty can apply the same keys, caret and selection as for a textarea. Reflect user edits into the underlying text nodes so the text survives a change of editability. A general browser would also need rich editing and inline descendant styles.

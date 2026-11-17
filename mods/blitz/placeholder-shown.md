---
title: :placeholder-shown matches, following the field's text
patch: 0097-blitz-dom-match-placeholder-shown-following-the-fiel.patch
upstream-fit: yes
---

Blitz matched `:placeholder-shown` as false for every element. A field
showing its placeholder was styled as one holding text, and a `:has()` over
it never changed.

`Node::is_placeholder_shown` reads the field. It matches an `input` of a
type that has a placeholder, or a `textarea`, whose `placeholder` is not
empty and whose value is empty. The value is the editor's text, or before
the field has an editor, the text the editor will start with. The document
keeps a set of its `input` and `textarea` elements. Before each style pass
it brings their `PLACEHOLDER_SHOWN` state (Stylo's state flag for the
pseudo-class) up to date, and snapshots a styled field whose state flips,
so that the selectors over it are invalidated. A field's text changes in
many places (typing, script, the embedder setting it), and checking before
each pass covers them all, at a cost of one look per field. The state
also agrees with the match in the first style pass. Without that, Stylo's
style sharing, which compares element state, gave a field with text the
style of a neighbour showing its placeholder.

We carry it for hotty-surf's kit. Basecoat's command palette hides its
separators with `.command:has(>header input:not(:placeholder-shown))
[role=separator]`, so in Blitz they were always hidden. Upstream should
want it; Chromium and Firefox match the pseudo-class from the live value
the same way.

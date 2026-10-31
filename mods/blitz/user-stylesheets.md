---
title: "Stylesheets of the user origin"
patch: 0015-blitz-dom-stylesheets-of-the-user-origin.patch
upstream-fit: yes
---

**What it changes:** `DocumentConfig::user_stylesheets`, and
`BaseDocument::add_user_stylesheet` and `remove_user_stylesheet`. A user
stylesheet wins over the user agent's and loses to the document's own,
whatever the selectors' specificity, as the CSS cascade orders its
origins.

**Why we carry it:** hotty's host stylesheet (SPEC §8) dresses controls,
focus, links and selection in the terminal's colours with
zero-specificity `:where()` rules, which must beat the UA sheet and lose to
the document. As a second UA sheet it lost to `input { background-color:
Field }`.

**For upstream:** yes. User stylesheets are part of CSS, and any embedder
with a theme can use them.

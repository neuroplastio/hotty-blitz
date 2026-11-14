---
title: "Lay out inline edges without interrupting whitespace"
patch: 0006-Lay-out-inline-edges-without-interrupting-whitespace.patch
upstream-fit: maybe
---

Allow a non-atomic inline edge to reserve width and a wrap opportunity without inserting a U+FFFC character into shaped text. Whitespace still collapses across the edge, and a zero-width edge adds no wrap opportunity. Blitz needs this to lay out span margins, borders and padding. Upstream could use a native span-edge model with the same whitespace behaviour.

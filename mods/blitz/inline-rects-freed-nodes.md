---
title: "An inline's client rects skip the text of freed nodes"
patch: 0045-blitz-dom-an-inline-s-client-rects-skip-the-text-of-.patch
upstream-fit: yes
---

**What it changes:** `Node::inline_fragment_boxes`, which
`get_client_rects` and `get_client_bounding_rect` use for a non-atomic
inline, walks its inline root's text layout and, for each run, the DOM
parents of the node the run was styled by. Between a mutation and the
next resolve that layout is the last one, and a run can name a node
freed since: `Node::with` panicked on it. A freed node is now not in the
target, and the walk stops there. A test in `document.rs` frees a
neighbour's text and asks again.

**Why we carry it:** hotty-blitz repaints only what a delta changes. It
asks where an element was before applying a change to it, while the
layout is the last frame's, and an earlier delta in the same frame may
have removed text beside it. hotty-blitz's fuzz test found the panic.

**For upstream:** yes. It is a one-line change, and any embedder that
asks for an inline's geometry before resolving can hit it. Upstream may
prefer to resolve layout before a geometry query, as a browser does.

**Platform:** neutral. It is Blitz code only, the same on macOS.

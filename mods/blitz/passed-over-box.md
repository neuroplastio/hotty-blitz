---
title: "A node construction passes over is built afresh"
patch: 0046-blitz-dom-a-node-construction-passes-over-is-built-a.patch
upstream-fit: yes
---

**What it changes:** construction builds no box for some nodes: an
inline its inline root's layout takes in, a table's row, a child of a
table that is not a cell. It spent their construction damage and kept
what they had built as boxes, down their subtree. Nothing builds those
while the node is passed over, and the damage a delta leaves there is
cleared with the rest, so they named the nodes it freed and none it
added. A box again (its table's `display` removed, say), the node had no
damage and resolve kept them: Taffy laid out a freed node and paint read
one. Now a passed-over node forgets its layout children, and theirs down
the boxes it owned, each flagged; a flagged layout child is built afresh
with its subtree, as a new one is. Three Blitz tests: a block in a table
whose child is replaced, a block under one, and an inline taken into an
inline layout.

**Why we carry it:** a delta under a block in a `display: table` button,
then the table removed, panicked in layout or paint. hotty-blitz 0.5.8
has it too. hotty-blitz's fuzz test found it at 3000 seeds.

**For upstream:** yes. It is a crash fix. Blitz's own
`assert_layout_parents_consistent` fails in debug builds on all three
tests without it. The hidden-box patch (0024) forgets a `display: none`
box's subtree the same way.

**Platform:** neutral. It is Blitz code only, the same on macOS.

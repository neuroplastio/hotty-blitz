---
title: Coalesce style element replacement parses
patch: 0064-Coalesce-style-element-replacements.patch
upstream-fit: no
source: kitesurf scripts/patches/blitz-dom-0.3.0-beta.4/0055-reparse-replaced-style-sheets-once-per-resolve.patch
---

Parse a registered style element's latest text once at resolve instead of
once per hotty text delta. First registration still starts imports and font
loads immediately, and removal discards queued work. Consecutive delta tests
cover the parse count, final rule and removal. Hotty has no JavaScript or
stylesheet reads between delta operations; other Blitz embedders may read
CSSOM synchronously and would need a separate flush before those reads.

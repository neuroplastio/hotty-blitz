# mods/parley on Parley's main

`make canary` is red for mods/parley on Parley's main, and stays red
until Blitz moves to a Parley release past 0.12. It was first red on
2026-10-31, at main 2e2163a (37 commits past v0.12.0). Nothing else is
wrong: mods/parley applies to its base, v0.12.0, the release Blitz pins,
which `make mods` checks.

On that main, three of the four patches conflict, each with an upstream
rework:

- 0001, emoji draw in colour whatever the font stack:
  `parley/src/shape/mod.rs`, where #911 takes (de)composition out of
  `AnalysisDataSources`.
- 0002, inline boxes follow their span's white-space:
  `parley/src/layout/data.rs` and `line_break.rs`, where #902 represents
  inline boxes with U+FFFC object replacement characters.
- 0003, the system is asked for a fallback once:
  `fontique/src/collection/mod.rs`.

0004 applies. canary.sh reports only the first conflict, 0001's.

When Blitz moves to the next Parley release, move mods/parley's base to
it with Blitz (mod.md), port the three, and delete this note.

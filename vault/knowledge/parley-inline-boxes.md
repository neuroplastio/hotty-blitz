# Parley inline boxes after 0.12

Parley commit `26052b9` represents in-flow inline boxes as U+FFFC object
replacement characters in shaped text. Its `Layout::edit_inline_box` rejects
a change between in-flow and out-of-flow after shaping. The existing Blitz
inline-edges patch builds span-edge boxes out of flow so whitespace collapses
across the span, then switches them in flow during layout when an edge has
width. A direct port panicked at Parley `layout.rs:184` in two select-control
host tests.

Moving forward requires either a Parley/Blitz redesign that preserves that
whitespace behavior and the zero-width wrap behavior, or a deliberate change
to those behaviors. That decision belongs to the maintainer. The rewrite
arrived before the Parley 0.13 tag: Blitz `c27b4ca` is the last commit before
Blitz moved from the published 0.12 release to a newer Parley git revision
that already includes it. The later version bump is not the boundary.

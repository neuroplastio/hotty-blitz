# The parley fork

The patches here are the whole fork: commits on upstream parley
`332c1c7d9758839577e7979d211d1e3bbf4818af`, the revision Blitz pins.
`Cargo.toml` swaps it in with `[patch."https://github.com/linebender/parley"]`
path dependencies on `../parley`, made by `scripts/parley-fork.sh`. Like the
Blitz fork, it stays local.

| patch | what |
| --- | --- |
| `0001-parley-*` | a cluster with emoji presentation takes a colour font before a text font (below) |

**Colour emoji first.** For a cluster with emoji presentation (an
Emoji_Presentation character, VS16, a keycap, a flag, a ZWJ sequence),
parley queried the style's font stack and then the emoji generic family,
and took the first font that covered it. A text font with outlines for the
character, as DejaVu Sans has for the faces, or the `monospace` generic,
then drew it in monochrome. The patch takes the first font with colour
glyphs (`COLR`, `CBDT`, `sbix` or `SVG `) that covers the cluster, from the
stack or the emoji family, and falls back to the old choice only when none
does, as browsers and Ghostty's own font resolver do. Text presentation
(VS15, or a text-default character such as U+263A alone) is unchanged.
Upstream main (2026-10-14) still appends the emoji family last.

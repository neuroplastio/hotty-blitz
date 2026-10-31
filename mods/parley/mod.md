---
upstream: Parley
url: https://github.com/linebender/parley
license: Apache-2.0 OR MIT
base: 6e1143dda817360dd3a2a64f9133c6e5dfabd881  # v0.12.0
builds: hotty-blitz
public: true
---

Blitz lays out text with Parley 0.12 from crates.io, and our `base` is the
commit tagged v0.12.0, which those packages were published from.
hotty-blitz draws hotty surfaces, often with a terminal's monospace font
stack, and Parley chose a font for emoji in a way that drew them in
monochrome there. It also broke lines between inline boxes that
`white-space: nowrap` keeps together, asked the system for a fallback font
again for every surface's copy of a font collection, and matched a font
stack again for every text layout. We mod Parley rather than fork it so
that each fix stays one change on the version Blitz builds with, until
Parley has its own.

In hotty-blitz, `make blitz`, which every build runs, sets up `../parley`:
Parley at `base`, branch `hotty`, with the set applied. `Cargo.toml`'s
`[patch.crates-io]` builds against it. `make mods` checks the set and that
it applies on `base`; `make canary` applies it to Parley's latest `main`.
After a change in the fork, export the patches into this directory again
with `git format-patch`, and `scripts/mods.py relink` points each `.md` at
its new file name.

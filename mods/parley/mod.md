---
upstream: Parley
url: https://github.com/linebender/parley
license: Apache-2.0 OR MIT
base: 332c1c7d9758839577e7979d211d1e3bbf4818af
builds: hotty-blitz
public: true
---

Blitz lays out text with Parley, at the revision Blitz pins, which is our
`base`. hotty-blitz draws hotty surfaces, often with a terminal's
monospace font stack, and Parley chose a font for emoji in a way that drew
them in monochrome there. It also broke lines between inline boxes that
`white-space: nowrap` keeps together, asked the system for a fallback
font again for every surface's copy of a font collection, and matched a
font stack again for every text layout. We mod Parley rather than fork it so
that each fix stays one change on the revision Blitz builds with, until
Parley has its own.

In hotty-blitz, `make blitz`, which every build runs, sets up `../parley`:
Parley at `base`, branch `hotty`, with the set applied. `Cargo.toml`'s `[patch]`
builds against it. `make mods` checks the set and that it applies on
`base`; `make canary` applies it to Parley's latest `main`. After a change in the fork,
export the patches into this directory again with `git format-patch`, and
`scripts/mods.py relink` points each `.md` at its new file name.

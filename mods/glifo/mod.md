---
upstream: glifo
url: https://github.com/linebender/vello
license: Apache-2.0 OR MIT
base: 0.4.0
builds: hotty-blitz
public: true
---

vello_cpu draws text with glifo, Linebender's glyph renderer from the vello
repository, and builds against glifo 0.4.0 from crates.io: the published
package, not the vello repository, for the reason vello_cpu's set gives,
since its dependencies are the crates.io ones vello_cpu builds against.
hotty-blitz draws a terminal program's surfaces beside the terminal's own
cells, with the same colour emoji font, and glifo drew an emoji's bitmap,
several times larger than the text at a terminal's sizes, with a filter
that skips most of its pixels, so its edges stair-stepped beside the
cells' and a browser's. We mod glifo rather than fork it so that the fix
stays one change on the version vello_cpu builds with.

In hotty-blitz, `make blitz`, which every build runs, sets up `../glifo`:
the package as published, checked against the checksum `Cargo.lock` had
for it, as the commit tagged `published` on branch `hotty`, with the set
applied. `Cargo.toml`'s `[patch.crates-io]` builds against it. `make mods`
checks the set and that it applies on `base`; `make canary` applies it to
glifo's latest release. After a change in the fork, export the patches
with `git format-patch --zero-commit`, since the base commit is made at
setup, and `scripts/mods.py relink` points each `.md` at its new file
name.

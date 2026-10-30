---
upstream: vello_cpu
url: https://github.com/linebender/vello
license: Apache-2.0 OR MIT
base: 0.1.0
builds: hotty-blitz
public: true
---

Blitz paints through anyrender, and hotty-blitz renders surfaces on the CPU
with anyrender's vello_cpu backend, which brings vello_cpu 0.1.0 from
crates.io: the published package, not the vello repository, because its
dependencies are the crates.io ones anyrender builds against. A terminal
repaints small parts of its surfaces many times a second, which is where
the first two patches and the fourth save; the third makes a large box
shadow cheap, and the fifth a clip nothing is drawn in. We mod vello_cpu
rather than fork it so that they stay five changes on the version we
build with. The first four still apply to vello_cpu 0.3.0. The fifth
needs its hook in 0.3.0's two new clip methods as well, so it will need
a version of its own there.

In hotty-blitz, `make blitz`, which every build runs, sets up
`../vello_cpu`: the package as published, checked against the checksum
`Cargo.lock` had for it, as the commit tagged `published` on branch
`hotty`, with the set applied. `Cargo.toml`'s `[patch.crates-io]` builds
against it. `make mods` checks the set and that it applies on `base`;
`make canary` applies it to vello_cpu's latest release. After a change in
the fork, export the patches with `git format-patch --zero-commit`, since
the base commit is made at setup, and `scripts/mods.py relink` points each
`.md` at its new file name.

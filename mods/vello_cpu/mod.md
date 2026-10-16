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
these two patches save. We mod vello_cpu rather than fork it so that they
stay two changes on the version we build with. The maintainer decided
(2026-10-10) not to offer them upstream, so both are rated `no`.

In hotty-blitz, `make blitz`, which every build runs, sets up
`../vello_cpu`: the package as published, checked against the checksum
`Cargo.lock` had for it, as the commit tagged `published` on branch
`hotty`, with the set applied. `Cargo.toml`'s `[patch.crates-io]` builds
against it. `make mods` checks the set and that it applies on `base`;
`make canary` applies it to vello_cpu's latest release. After a change in
the fork, export the patches with `git format-patch --zero-commit`, since
the base commit is made at setup, and `scripts/mods.py relink` points each
`.md` at its new file name.

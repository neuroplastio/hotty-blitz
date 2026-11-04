# A nest's forks go stale

A nest is a directory that holds a hotty-blitz worktree beside its forks
(`../blitz`, `../parley`, `../stylo`, `../vello_cpu`, `../glifo`), which
`make check` builds against. The forks are set up once and then kept, so
a patch added to `mods/` reaches the build only if the fork gets it too.

- parley, vello_cpu, glifo and Stylo: `make blitz` compares the fork's
  commits on its base with the set's patch count and runs the fork script
  when they differ. The script applies the missing patches to a clean fork
  whose commits match the first patches by subject (`parley-fork.sh`).
- Blitz: `make blitz` only checks that `../blitz` exists, and
  `blitz-fork.sh` on an existing fork reports its count and changes
  nothing. A new `mods/blitz` patch is therefore not in the build, and
  `make check` passes without it.

That happened on 2026-11-04. hotty-blitz 0.6.5's first gate ran on a nest
whose Blitz fork lacked mods/blitz 0050 (no hinting above a scale of 1).
The gate was green and the text was unchanged, so its measurement came out
byte-identical to 0.6.4's. The patch was applied to the fork with `git am`
and the gate run again before the hottyterm pin.

Until `make blitz` catches Blitz up as it does the others, after adding or
changing a `mods/blitz` patch in a nest, check
`git -C ../blitz rev-list --count <base>..hotty` against
`ls mods/blitz/*.patch | wc -l`, and bring the fork level (`git am` the
missing patches, or set it up again) before trusting the gate.

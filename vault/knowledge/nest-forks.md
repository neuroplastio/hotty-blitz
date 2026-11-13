# A nest's forks go stale

A nest is a directory that holds a hotty-blitz worktree beside its forks
(`../blitz`, `../parley`, `../stylo`, `../vello_cpu`, `../glifo`), which
`make check` builds against. The forks are set up once and then kept, so
a patch added to `mods/` reaches the build only if the fork gets it too.

- parley, vello_cpu, glifo and Stylo: `make blitz` compares the fork's
  commits on its base with the set's patch count and runs the fork script
  when they differ. The script applies the missing patches to a clean fork
  whose commits match the first patches by subject (`parley-fork.sh`). The
  vello_cpu workspace applies the vello_cpu set followed by glifo's set;
  the separate glifo fork checks that set on its own.
- An old single-package `../vello_cpu` or `../glifo` is not converted: move
  it aside (for example `mv ../vello_cpu ../vello_cpu.old` and likewise for
  glifo), then rerun `make blitz` to create the vello workspaces.
- When a fork's `hotty` branch does not descend from the new set's base,
  `make blitz` stops rather than applying patches to the wrong base. Move
  that fork aside and rerun it. For the c27b4ca rebase, older Blitz and
  Stylo forks need this too; a clean Parley 0.12 prefix can be caught up.
- Blitz: the same, since 2026-11-04. Before that, `make blitz` only checked
  that `../blitz` existed, and `blitz-fork.sh` on an existing fork
  reported its count and changed nothing, so a new `mods/blitz` patch was
  not in the build and `make check` passed without it. Now a fork that
  cannot be caught up (ahead, dirty, off `hotty`, or other subjects) fails
  `make blitz` with what it found.

That happened on 2026-11-04. hotty-blitz 0.6.5's first gate ran on a nest
whose Blitz fork lacked mods/blitz 0050 (no hinting above a scale of 1).
The gate was green and the text was unchanged, so its measurement came out
byte-identical to 0.6.4's. The patch was applied to the fork with `git am`
and the gate run again before the hottyterm pin.

A changed patch (same count, new content) is still not caught: after
changing a `mods/blitz` patch in a nest, change the fork the same way, or
set it up again, before trusting the gate.

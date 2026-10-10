# The vello_cpu fork

The patches here are the whole fork: commits on the vello_cpu 0.1.0 package
as published on crates.io, the version anyrender_vello_cpu brings. Cargo.lock
had it with sha256 `ac7349e1…a93ea`, which `scripts/vello-fork.sh` checks
before it unpacks the package at `../vello_cpu` (the commit tagged
`published`, branch `hotty`) and applies the patches. `Cargo.toml` swaps it in
with `[patch.crates-io]`. The published package rather than the vello
repository: its dependencies are the crates.io ones anyrender_vello_cpu builds
against, where the repository's would be its own path crates.

Like the Blitz fork it stays local. After changing it, export the patches
again: `git -C ../vello_cpu format-patch -N -o "$PWD/vello" published..hotty`.
A new vello_cpu (an anyrender_vello_cpu upgrade) means a new `VERSION` and
`SHA256` in the script and the patches rebased onto it; upstream's 0.3.0
(2026-10-02) still spins as 0001 describes.

| patch | what |
| --- | --- |
| `0001-vello_cpu-*` | flushing waits for the workers without spinning (below) |

## 0001: flushing waits for the workers without spinning

`MultiThreadedDispatcher::flush` drained the workers' recorded commands by
looping on `try_recv` until every worker had hung up, so the painting thread
spun at full speed for as long as the workers took on the last batch: on
`dash.py` (10 Hz, about 0.45 Mpx of 3.5 Mpx changing a frame, 8 workers) a
quarter of hotty-blitz's instructions. It now blocks in the ordered channel's
`recv`, which hands out the tasks in the same order and fails once the
senders are gone and nothing is buffered.

Measured on `dash.py` in hottyterm, counting instructions with `perf` (gov
R-5): the painting thread −39%, the process −23%. `hotty bench` on an idle-ish
Ryzen 5 5600X: frame times unchanged (one cell 0.68 ms either way).

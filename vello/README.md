# The vello_cpu fork

The patches here are the whole fork: commits on the vello_cpu 0.1.0 package
as published on crates.io, the version anyrender_vello_cpu brings. Cargo.lock
had it with sha256 `ac7349e1…a93ea`, which `scripts/vello-fork.sh` checks
before it unpacks the package at `../vello_cpu` (the commit tagged
`published`, branch `hotty`) and applies the patches. `Cargo.toml` swaps it in
with `[patch.crates-io]`. The published package rather than the vello
repository: its dependencies are the crates.io ones anyrender_vello_cpu builds
against, where the repository's would be its own path crates.

Like the Blitz fork it stays local, and the maintainer decided (2026-10-10)
not to offer these patches upstream. After changing the fork, export the
patches again, with zero hashes since the base commit is made at setup:
`git -C ../vello_cpu format-patch -N --zero-commit -o "$PWD/vello" published..hotty`.
A new vello_cpu (an anyrender_vello_cpu upgrade) means a new `VERSION` and
`SHA256` in the script and the patches rebased onto it; upstream's 0.3.0
(2026-10-02) still spins as 0001 describes.

| patch | what |
| --- | --- |
| `0001-vello_cpu-*` | flushing waits for the workers without spinning (below) |
| `0002-vello_cpu-*` | a render context takes at most 4 workers by default (below) |

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

## 0002: at most 4 workers

The default was one worker per core but one, up to 8. A 300-frame `dash.py`
replay and `hotty bench` with the count overridden (gov R-5, Question 5):

| workers | 5600X replay paint / CPU | 5600X full paint | laptop replay paint / CPU | laptop full paint |
| --- | --- | --- | --- | --- |
| 8 | 2.13 ms / 2.87 s | 7.6–8.7 ms | 1.76 ms / 2.19 s | 7.3–8.1 ms |
| 4 | 1.79 ms / 1.86 s | 7.9–8.3 ms | 1.42 ms / 1.38 s | 8.0–8.7 ms |

Past 2 workers a full paint barely gets faster (the main thread's share
dominates), while each worker adds dispatch and idle spinning to every small
paint. The maintainer chose 4 (2026-10-10): multithreading stays for full
paints. It also halves the threads of a host's 8 shared contexts (64 → 32).

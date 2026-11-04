# What a CSS animation's frames cost

From 0.6.7 a CSS animation asks for a frame when its output next changes
(mods/stylo 0003, mods/blitz 0052), not every 60th of a second while it
is active. Measured 2026-11-04 on plx's toast countdown bar: 3px tall,
300px wide, `transform: scaleX(1 → 0)` over 60 s, at scale 2.

`examples/anim_cost` (no terminal; 3 alternating rounds of 10 s each,
user-space instructions, loads 2.5–15). Before is 3929dc6 (0.6.6),
after is 9369d23:

| bar | wakes/s | frames/s | px a frame | instructions/s |
| --- | --- | --- | --- | --- |
| none, or static | 0 | 0 | — | 15k, both |
| `steps(600)`, before | 59.5 | 10.0 | 24 016 | 6.79–6.85M |
| `steps(600)`, after | 10.0 | 10.0 | 24 016 | 3.74M |
| `linear`, before | 59.4 | 59.4 | 24 016 | 22.3M |
| `linear`, after | 59.0 | 59.0 | 24 016 | 22.1–22.2M |
| `linear` in a 30 s delay, before | 59.2 | 0 | — | 3.39–3.43M |
| `linear` in a 30 s delay, after | 0 | 0 | — | 16k |

Each frame painted only the bar's box and its margin, never the whole
surface; between steps a resolve found no style change and painted
nothing, at about 60k instructions a wake.

In hottyterm (the installed 26.11.03 against this library through
`LD_LIBRARY_PATH`, headless sway at 2400×1500 and scale 2, `perf stat`
over 10 s after 4 s, 3 alternating rounds), the same bar:

| bar | CPU | instructions per 10 s |
| --- | --- | --- |
| none | 0.2–0.4% | 8M |
| `steps(600)`, release library | 7.5–11.5% | 364–368M |
| `steps(600)`, 0.6.7 | 2.1–3.0% | 109M |
| `linear`, either | 11.4–13.0% | 586–595M |

The terminal paid more for an empty wake than hotty-blitz did: with the
release library its GTK thread took 199M and its renderer 77M of the
365M, though only 10 frames a second drew anything. A smooth `linear`
bar still costs its 60 frames; no cadence cap was added for it.

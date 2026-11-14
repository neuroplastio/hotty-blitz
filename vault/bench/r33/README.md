# R-33 leg 2: timing kitesurf's perf patches

gov R-33 leg 2 keeps a perf patch only if it shows a win on our documents
and leaves the pixels unchanged. This directory holds the method, the
documents, and run summaries. Raw TSVs stay on hotty-perf local branch `perf33/base`.

## What is timed

`hotty render --repeat N` (crates/hotty, added for this): after the first
frame, N more paints of the placed document. Each is a full repaint
(`Host::repaint`), or with `--wheel DY --at X,Y` one wheel step on a
scrolling document (sent with `scroll=3`), down and up in turn, which
damages only the scroll box. The number is the paint stage
(`Timings::paint_us`: scene and raster, 4 vello workers); style and
layout are not touched by a repaint and are not compared.

## Documents

- The hotty corpus (`corpus/*.html`, hotty adf18a5), as `make corpus`
  renders it: 80 columns, scale 2, 20×42 px cells.
- From R-22's rig (hottyterm `scripts/feel/prog`, 91fddb3,
  `feelprog html <name> --cols 120 --rows 35 --cell 10x21`; e6a/e6b take
  no flags): e1p, the dashboard (gradient meters, rounded clipped cards);
  e3, the hover list (overflow:hidden rows); e6a and e6b, the two 60 KB
  storybook pages (a gradient banner). Generated to
  `~/.cache/hotty-perf/r33/docs/`.
- Written here for the patches' cases: `shadows.html` (24 cards, blurred
  shadows 4–48 px, radius 0–12, every fifth inset), `gradients.html`
  (30 distinct three-stop gradients on a radial backdrop),
  `gradients-same.html` (30 bars with one gradient, the LUT reuse case),
  `scroll.html` (a 300-row scroll box of overflow:hidden rows), timed both
  as full repaints and as wheel steps (`scroll-wheel`).
- All but the corpus at scale 1.6 on 120×35 cells of 16×34 px.

## How a run is done

```
systemd-run --user --scope -q -p MemoryMax=12G -p MemorySwapMax=2G \
  -p CPUWeight=idle -p AllowedCPUs=4-11,16-23 -- \
  flock -o ~/.cache/hotty/sway-feel/lock \
  vault/bench/r33/run.sh OUT ROUNDS FRAMES base=BIN base-aa=COPY patched=BIN
vault/bench/r33/sum.py OUT base patched base-aa
```

- Each build is a release binary copied out of its worktree
  (`~/.cache/hotty-perf/r33/bin/<label>/`). Every round runs each build
  once per document, ABBA order, one process of FRAMES paints each.
- The call: a patch wins on a document when all rounds but one are faster
  (sign test) and the median paired change is past 5% or twice the A/A
  control's on that document, whichever is larger. The A/A control (a
  copy of the base binary) is in every run because, at this laptop's load
  (8–16 during the baseline, from other sessions), rounds are correlated:
  the baseline's A/A alone passes the sign test with −6% (e1p) and −11%
  (e3).
- Pixels: `pixels.sh render DIR BIN` renders every document as timed,
  and `pixels.sh diff BASE OTHER` gives the pixels that differ and the
  largest channel difference. Two renders of the same build differ by 0
  pixels on every document, so any difference is the patch's.

## Baseline: 4efd678 (hotty-blitz 0.7.5), 2026-11-14

8 rounds × 60 frames, load 8.3–16.6. The raw load TSV stays on hotty-perf local branch `perf33/base`.
Paint stage, ms:

| document | frames | paint p50 ms | p10 | p90 | rounds' p50 |
| --- | ---: | ---: | ---: | ---: | --- |
| 01-card | 480 | 0.34 | 0.31 | 0.50 | 0.32–0.49 |
| 02-typography | 480 | 1.46 | 1.12 | 3.17 | 1.12–2.92 |
| 03-grid | 480 | 0.89 | 0.69 | 2.14 | 0.73–1.76 |
| 04-table | 480 | 0.44 | 0.39 | 0.81 | 0.40–0.86 |
| 05-forms | 480 | 0.86 | 0.69 | 2.19 | 0.70–2.80 |
| 06-effects | 480 | 0.86 | 0.70 | 4.96 | 0.69–6.61 |
| 07-unicode | 480 | 1.29 | 0.89 | 9.12 | 0.90–8.79 |
| 08-selectors | 480 | 0.94 | 0.65 | 5.24 | 0.66–7.26 |
| 09-overflow | 480 | 0.41 | 0.34 | 1.16 | 0.35–1.17 |
| 10-svg-images | 480 | 0.15 | 0.14 | 0.22 | 0.14–0.18 |
| 11-svg-current-color | 480 | 0.36 | 0.28 | 0.71 | 0.28–0.65 |
| e1p | 480 | 3.94 | 2.59 | 18.43 | 2.52–23.62 |
| e3 | 480 | 3.08 | 1.86 | 16.47 | 1.83–17.28 |
| e6a | 480 | 4.40 | 3.18 | 16.10 | 3.26–19.78 |
| e6b | 480 | 4.12 | 3.15 | 8.71 | 3.27–8.17 |
| gradients | 480 | 3.25 | 2.51 | 5.79 | 2.62–6.15 |
| scroll | 480 | 3.96 | 3.25 | 7.72 | 3.30–8.25 |
| scroll-wheel | 480 | 3.28 | 2.78 | 5.84 | 2.82–6.32 |
| shadows | 480 | 14.70 | 13.34 | 32.24 | 13.45–29.50 |

Sub-millisecond pages (most of the corpus) swing ±20% between rounds and
cannot show a patch's win here; the documents that can are shadows
(14.7 ms), e1p, e3, e6a/b, gradients and scroll (3–4 ms). With the 5% bar
alone the A/A run would call e1p and e3 wins; the control's own bar
(twice its paired median) puts those two at about 11% and 21% for this
load, so a patch run on a quieter machine sets its own.

## The patches: kitesurf-perf-075 (0b55b57), 2026-11-14

Each patch cherry-picked alone on this branch's harness (branches
`perf33/shadow`, `perf33/lut`, `perf33/clip`, and `perf33/all` with the
three), with fork branches `v-<name>` in `../vello_cpu` and `v-clip` in
`../blitz`; the shadow fill with vello_cpu's `poly-pnorm` turned on (a
measure-only commit). The base rebuilt byte-identical to the baseline's.
10 rounds × 60 frames, six builds (base, A/A copy, four patched), load
4.3–12.4; the A/A control called nothing (largest paired median 3.1%).
Full tables: `runs/patches-0b55b57/summary.md`.

| patch | wins (paired median, 10/10 rounds) | losses | pixels | verdict |
| --- | --- | --- | --- | --- |
| shadow fill (07a2d5b) | every shadowed page: dialogs −65/−70/−72% at 1/1.6/2 (18.9 → 5.6 ms at 1.6), shadows −65/−67/−70% (13.3 → 4.4), ks-shadows −67 to −72%, ks-wide-shadows −74%, 06-effects −32% | none | ≤ 1/255 per channel; 22–213 pixels a page on shadowed pages (ks-shadows 54/148/213, ks-wide 34/99/156 at 1/1.6/2); corpus identical | keep |
| gradient LUT (f54c511) | dots −30% (6.1 → 4.3 ms) | none | identical on every page | keep |
| clip paths (baa0bf3) | none | clips@2 +5.8% (1/10 faster), ks-gradients@1 +7.1% (1/10; 0.20 ms) | identical on every page | drop |
| all three | the shadow and LUT wins, unchanged | ks-gradients@1 +7.7% (0/10), from the clip patch | as the shadow fill | — |

The LUT patch does nothing for `gradients-same` (bars of one linear
gradient, 1.50 ms either way): its win is the dot grid's repeated radial
tiles. Whether `clips.html` reaches the new clip path is not checked from
here (pixels are identical either way); on our documents it is no gain.

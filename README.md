# hotty-blitz

[HOTTY](https://github.com/neuroplastio/hotty) on Blitz. Blitz assembles
Firefox's style engine (Stylo), html5ever, Taffy for layout and Parley for
text, and renders on the CPU. This repository holds:

| crate | what |
| --- | --- |
| `crates/hotty-blitz` | the host: surfaces, patches and morph, `cid:` resources, damage-proportional rendering into a caller's buffer, input and events, and a **C ABI** (`include/hotty_blitz.h`) for terminals to link |
| `crates/hotty-wire` | the envelope: a stream scanner that passes terminal bytes through untouched and yields HOTTY commands, plus the encoder |
| `crates/hotty` | the `hotty` CLI: `render` and `show` a page, `run` (the kitty graphics polyfill), `send`, `dump`, `replay`, `bench`, `css` |
| `blitz/` | the patch on Blitz that makes a patch cost the depth of the tree rather than its size ([blitz/README.md](blitz/README.md)) |

## Try it

Needs [mise](https://mise.jdx.dev) and a checkout of `neuroplastio/hotty`
next to this repository (or `HOTTY_DIR`).

```
mise install && make release       # also clones and patches Blitz at ../blitz
./target/release/hotty run -- python3 ../hotty/examples/dash.py
./target/release/hotty render ../hotty/corpus/01-card.html -o card.png
```

`hotty run` works in any terminal with kitty graphics (kitty, Ghostty, and
others). It sits on the pty, passes everything through except HOTTY, and shows
surfaces as images (SPEC §14). It also works across SSH:
`hotty run -- ssh host program`.

A terminal can instead link `libhotty_blitz` through the C ABI and render
surfaces natively: sharp at any zoom, no pixels on the wire, and local
interaction. [hottyterm](https://github.com/neuroplastio/hottyterm), a fork of
Ghostty, does this as a proof of concept of the protocol; it stops existing
if Ghostty gains HOTTY support.

## Performance

Measured 2026-09-29, release build:

- **A one-cell patch** costs about 0.1 ms per frame at any size of nested
  document (64 to 262,144 cells), because the Blitz patch makes construction,
  layout and rounding stop where the change stops.
- **Paint** covers only the damage: the old and new boxes of what changed.
- **Bub-n-Bros** (`../hotty/examples/bubbros.py`) patches 7 of its ~380
  sprites a frame.

Flat lists still cost linear time in their length: every pass, Stylo's
included, scans the children of the node that changed.

## Fidelity

[docs/fidelity/](docs/fidelity/) has the corpus rendered here next to
Chromium. The known gaps:
- forms (`type=password` shows its text);
- tables (captions, `colspan`);
- `text-overflow: ellipsis`;
- container queries;
- colour emoji;
- 3D transforms.

`make oracle` rebuilds the sheets.

## Environment

| variable | effect |
| --- | --- |
| `HOTTY_LOG=<file>` | log what `hotty run` does |
| `HOTTY_FRAME_LOG=<file>` | one line of timings per rendered frame (in `hotty run` and any terminal linking the library) |
| `HOTTY_STATS=1` | a summary when `hotty run` exits |
| `HOTTY_SCALE=<s>` | device pixels per CSS pixel, overriding the guess |

## Licence

Apache-2.0 ([LICENSE](LICENSE)).
- Blitz is MIT OR Apache-2.0, and the patch in `blitz/` is Apache-2.0.
- Stylo is MPL-2.0 and stays a dependency.

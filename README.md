# hotty-blitz

[HOTTY](https://github.com/neuroplastio/hotty) on Blitz. Blitz assembles
Firefox's style engine (Stylo), html5ever, Taffy for layout and Parley for
text, and renders on the CPU. This repository holds:

| crate | what |
| --- | --- |
| `crates/hotty-blitz` | the host: surfaces, deltas and morph, `cid:` resources and the network policy (animated GIF, APNG and WebP play), damage-proportional rendering into a caller's buffer, input and events, and a **C ABI** (`include/hotty_blitz.h`) for terminals to link |
| `crates/hotty-wire` | the envelope: a stream scanner that passes terminal bytes through untouched and yields HOTTY commands, plus the encoder |
| `crates/hotty` | the `hotty` CLI: `render` and `show` a page, `run` (the kitty graphics polyfill), `send`, `dump`, `replay`, `bench`, `css` |
| `blitz/` | the patches on Blitz: among them, a delta costs the depth of the tree rather than its size ([blitz/README.md](blitz/README.md)) |

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
interaction.

## The network

A surface fetches nothing from the network unless its user allows it
(SPEC §7.2). The terminal's half of the policy is CSP's syntax:
`hotty run --net 'img-src https:'`, `hotty_host_set_network` through the C
ABI. The capabilities report it as `net`. A document asks with
`<meta name="hotty-network" content="img-src https://example.com">`, and a
URL is fetched only when a source in both halves matches it, for its
directive: `img-src` for images (`<img>`, CSS backgrounds and masks),
`style-src` for stylesheets and `@import`, `font-src` for `@font-face`.

- Only `http` and `https`, never a document (`<iframe>`), never a file, and
  nothing under the default base `https://hotty.invalid/`. Relative URLs
  resolve against the document's `<base>`.
- No referrer, cookies or credentials. A redirect is followed only where
  the policy allows its target too.
- At most 8 MiB and 10 seconds per fetch, its redirects included; past
  either, it fails as a missing resource does.
- Four threads fetch for the whole process, off every terminal's render
  thread, into one cache (64 MiB, least recently used out): a URL shown in
  several surfaces or terminals is fetched once while it is cached. What
  arrives wakes the terminal (`hotty_host_set_waker`), and the surface is
  drawn again; a placement made with `f=1` hears `fit` if its height
  changed (SPEC §5.2). Animated GIFs from the network play.
- `srcset`, `sizes` and `<picture>` choose the source, with `cid:` too:
  the candidate for the display's density, the first `<source>` whose
  `type` and `media` match. They choose again when the display changes
  (another scale, a zoom, a light or dark theme, a new width for
  `sizes`).
- An inline SVG's `<image href>` is an image like any other: `cid:`, or
  fetched under `img-src`. An SVG that is itself an image (`<img>`, CSS)
  loads only `data:` URLs inside it, as in a browser.

## Scrolling

A document scrolls only along the axes it asks for (`a=doc` with
`scroll=1`, `2` or `3`, SPEC §5.3), as a page does: its root and its
`overflow: auto` and `scroll` boxes, with overlay scrollbars that take
pixels, never cells, and fade. Along an axis it did not ask for nothing
moves, whatever its CSS, and nothing shows a scrollbar.

- A terminal hands every wheel, touchpad scroll and touch drag to
  `hotty_host_wheel` first. It returns whether the surface took it; one
  it did not take is the terminal's, as over the cells. A gesture goes
  where its first wheel went, as in a browser: the innermost box that can
  still move that way, then the root, then the terminal, unless
  `overscroll-behavior` stops it.
- While a surface has the keyboard, the keys a browser scrolls with
  scroll it (those its focused control does not use), and focus scrolls
  an element into view.
- `hotty run` does the same with the wheel reports it reads: one report
  is a row.

## Performance

Measured 2026-09-29, release build:

- **A one-cell delta** costs about 0.1 ms per frame at any size of nested
  document (64 to 262,144 cells), because the Blitz patch makes construction,
  layout and rounding stop where the change stops.
- **Paint** covers only the damage: the old and new boxes of what changed.
- **Bub-n-Bros** (`../hotty/examples/bubbros.py`) sends deltas to 7
  of its ~380 sprites a frame.

Flat lists still cost linear time in their length: every pass, Stylo's
included, scans the children of the node that changed.

Not done yet:
- **Occlusion.** A placement wholly under others (SPEC §5.2, `z`) is still
  laid out and painted, and its deltas still cost a render. Overlap
  detection over the placements would let the host skip the paint of what
  cannot show, and defer its deltas until it shows again.

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

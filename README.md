# hotty-blitz

[HOTTY](https://github.com/neuroplastio/hotty) on Blitz. Blitz assembles
Firefox's style engine (Stylo), html5ever, Taffy for layout and Parley for
text, and renders on the CPU. This repository holds:

| crate | what |
| --- | --- |
| `crates/hotty-blitz` | the host of HOTTY 0.2: surfaces, deltas and morph, `cid:` resources and the network policy (animated GIF, APNG and WebP play), damage-proportional rendering into a caller's buffer, input and events (their bodies msgpack), and a **C ABI** (`include/hotty_blitz.h`) for terminals to link |
| `crates/hotty-wire` | the envelope: a stream scanner that passes terminal bytes through untouched and yields HOTTY commands, plus the encoder |
| `crates/hotty` | the `hotty` CLI: `render` and `show` a page, `run` (the kitty graphics polyfill), `send`, `dump`, `replay`, `bench`, `css` |
| `blitz/` | the patches on Blitz: among them, a delta costs the depth of the tree rather than its size ([blitz/README.md](blitz/README.md)) |
| `vello/` | the patches on vello_cpu, the renderer: flushing waits for its workers without spinning, and at most 4 of them ([vello/README.md](vello/README.md)) |

## Try it

Needs [mise](https://mise.jdx.dev) and a checkout of `neuroplastio/hotty`
next to this repository (or `HOTTY_DIR`).

```
mise install && make release       # also patches Blitz at ../blitz and vello_cpu at ../vello_cpu
./target/release/hotty run -- python3 ../hotty/examples/dash.py
./target/release/hotty render ../hotty/corpus/01-card.html -o card.png
./target/release/hotty dump capture.bin     # the HOTTY messages in a captured stream
```

`hotty run` works in any terminal with kitty graphics (kitty, Ghostty, and
others). It sits on the pty, passes everything through except HOTTY, and shows
surfaces as images (SPEC §14). It also works across SSH:
`hotty run -- ssh host program`.

A terminal can instead link `libhotty_blitz` through the C ABI and render
surfaces natively: sharp at any zoom, no pixels on the wire, and local
interaction. [hottyterm](https://github.com/neuroplastio/hottyterm) does
exactly that; install it with `brew install --cask neuroplastio/tap/hottyterm`
(macOS on Apple silicon) or the AUR's `hottyterm-bin` (Arch on x86_64).

## Reading the wire

A host's replies and events carry msgpack bodies (SPEC §3.3), in base64,
which no one reads off a terminal. `hotty dump [FILE]` reads a captured
stream from FILE or stdin, a program's output or what its terminal gave it
to read (util-linux's `script -I in.bin -O out.bin`), and prints one line
for each HOTTY message, its chunks joined and `o=z` inflated: its control,
then its body as JSON with each value's type in sight, or what a program
sent as its text.

```
a=ok:n=1:re=q  {"v": "0.2", "ops": ["morph", …], "cell": {"w": 20, "h": 42}, "scale": 2.0, …}
a=delta:s=form:op=text:t=status:q=2  saved ✓\n
a=ev:s=form:e=resize:t=  {"w": 500.0, "h": 147.0}
```

- A float always has a fraction or an exponent (`2.0`, `1e-7`), and an int
  never. Bytes are `h'…'` in hex, a timestamp (extension −1) `t'…'` in
  RFC 3339, another extension `ext(<type>, h'…')`.
- A body that is not one msgpack map says why after `  !`: not a map, bytes
  after it, nested deeper than 32 levels, or not msgpack. So does one that
  holds, anywhere in it, what no host sends (SPEC §3.3), which fails the
  whole body for its reader: a nil, a key that is not a string, a key
  given twice in one map (a fixstr and a str 8 of one name are one key), a
  string that is not UTF-8, an int further than 2^53 − 1 from zero, or a
  timestamp of another size, with a second's nanoseconds or more, or past
  2^53 − 1 seconds. Each kind is said once, at the byte it is first at
  (`  !a nil at byte 13, and 2 more`), and the body is shown all the same.
- In text, `\` and control characters are escaped (`\n`, `\e`); a
  resource that is not text shows its size and first bytes.
- `--all` prints the bytes between messages too, as `(bytes)` lines; a
  sequence that does not decode is an `(invalid)` line.

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

- A terminal hands every wheel, touchpad scroll and touch pan (below) to
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

## Touch

A terminal passes every phase of a touch that begins over a surface
(where `hotty_host_takes_pointer` is true) to `hotty_host_touch`
(`Host::touch`), taps included, and none of it to
`hotty_host_pointer`: down, move, up, cancel (a second finger, or the
platform cancelled it) and long press (the terminal took it for one).
hotty-blitz decides whose the touch is (SPEC §9.1) and says so on every
phase: the terminal's (0), undecided (1: hold its moves) or the
surface's (2).

- A touch drags an element with `drag` in its `data-on` and an id when the
  `touch-action` of the element touched allows no pan along its first
  move past the tap slop, 8 CSS px (`TAP_SLOP`, GTK's drag threshold and
  the addon's): the larger of the move's two deltas, a tie being a pan.
  The value is Pointer Events': the touched element's and its ancestors'
  up to the nearest element that scrolls, so `none`, `pinch-zoom` and a
  `pan-x` or `pan-y` across the move drag; `auto` and `manipulation`
  pan. Stylo parses no `pan-left`, `pan-right`, `pan-up` or `pan-down`.
- A drag is a press when it is decided: `press` and `dragstart` at the
  cell where the touch began, what the press causes (focus), and a
  `drag` at once if the finger is already over another element. Then
  drags as for a mouse, and its lift ends it as a mouse drag ends
  (`dragend`, and a `click` if it lifts over the element it began on). A cancel ends it
  with `dragend` and an empty target, and the rest of the gesture is the
  terminal's.
- A pan is the terminal's: it scrolls with it, `hotty_host_wheel` first,
  from where the touch began, and the pan presses nothing. Touches with
  Alt held at touch-down, on no drag element, on a detached surface or
  after a long press never drag. A tap, a touch that never went past the
  slop, is a click where the finger lifted: a press there, never a drag.
- A touch hovers nothing.

## Text fields

A focused text field types and edits only as its program's keymap says
(SPEC §10.2): the default keymap, then each `data-keys` from the root to
the field. A bound key does its action, a printable key without Control,
Alt or Meta types, and every other key goes to the program. Words, lines
and a password field are the spec's (`crates/hotty-blitz/src/edit.rs`);
moves by rows and pages follow the field's layout. A terminal offers a key
twice at most (SPEC §10.4): first as the user pressed it, named from the
key event, before its own shortcuts and translations
(`hotty_host_key_pressed`), so Command+ArrowLeft is Meta+ArrowLeft and not
the Control+a a macOS terminal sends for it; then, if the surface did not
use it, as the bytes its key encoding or a binding would write
(`hotty_host_key_bytes`), named by what the program would read
(`hotty_wire::keys`), so a remapped key is the key it sends. The keys a
field does not use go on to the program as they came. Any other focused
element's `data-keys` gives keys to the program, or, in a document that
scrolls, scrolls with them (`scroll-down`, `scroll-end`, …), after the keys
the element uses itself; a field leaves those scroll actions out.

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
- vello_cpu is Apache-2.0 OR MIT, and the patch in `vello/` is Apache-2.0.
- Stylo is MPL-2.0 and stays a dependency.

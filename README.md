# hotty-blitz

[hotty](https://github.com/neuroplastio/hotty) on Blitz. Blitz assembles
Firefox's style engine (Stylo), html5ever, Taffy for layout and Parley for
text, and renders on the CPU. This repository holds the following; the protocol itself, the envelope a
program's output is split by, a program's commands read and checked, and
the replies and events written, is the `hotty` crate of
[hotty-rs](https://github.com/neuroplastio/hotty-rs), which every Rust
host and program shares:

| crate | what |
| --- | --- |
| `crates/hotty-blitz` | the host of hotty 0.5: surfaces, deltas and morph, `cid:` resources and the network policy (animated GIF, APNG and WebP play), damage-proportional rendering into a caller's buffer, input and events (their bodies msgpack), and a **C ABI** (`include/hotty_blitz.h`) for terminals to link |
| `crates/hotty` | the `hotty` CLI (package `hotty-cli`): `render` and `show` a page, `run` (the kitty graphics polyfill), `send`, `dump`, `replay`, `bench`, `css` |
| [`mods/`](mods/) | our patches on Blitz, parley (its text layout) and vello_cpu (its renderer), one set each, every patch with a page on what it changes, why we carry it and whether upstream might want it (gov P-7; neuroplast.io/modding lists them) |

The host speaks hotty 0.5 and no other version. It answers a query that
lists 0.5 among the versions its program speaks (`a=q:n=1:v=0.5`, SPEC §4)
with its capabilities, and any other, one with no `v` too, with `EVERSION`:
a program of 0.4 finds no host.

0.5.x speaks hotty 0.5; the patch number is this repository's own. The
capabilities name it: `"host": "hotty-blitz"`, `"version": "0.5.6"`.

## Try it

Needs [mise](https://mise.jdx.dev) and a checkout of `neuroplastio/hotty`
next to this repository (or `HOTTY_DIR`).

```
mise install && make release       # also patches Blitz at ../blitz, parley at ../parley and vello_cpu at ../vello_cpu
./target/release/hotty run -- python3 ../hotty/examples/dash.py
./target/release/hotty render ../hotty/corpus/01-card.html -o card.png
./target/release/hotty dump capture.bin     # the hotty messages in a captured stream
```

`hotty run` works in any terminal with kitty graphics (kitty, Ghostty, and
others). It sits on the pty, passes everything through except hotty, and shows
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
for each hotty message, its chunks joined and `o=z` inflated: its control,
then its body as JSON with each value's type in sight, or what a program
sent as its text.

```
a=ok:n=1:re=q  {"v": "0.5", "cellcss": {"w": 10.0, "h": 21.0}, "scale": 2.0, …}
a=delta:s=form:q=2  {"ops": [{"op": "text", "t": "status", "v": "saved ✓"}]}
a=ev:s=form:e=fit  {"r": 3}
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
- `reveal` (SPEC §6.1) scrolls an element into view for the program: its
  top to the top of each box that holds it (`start`), its bottom to the
  bottom (`end`), or as little as it takes; across, as little as it
  takes. It scrolls only the root and `overflow: auto` and `scroll` boxes,
  never one that is `hidden`, and moves no focus.
- `view` (SPEC §9.5) tells the program which elements with
  `data-on~=view` and an id are in view, each with its area, and whether
  the root shows its end, from the frame that draws them: shown in the
  window and clipped by no box on the element's chain of containing
  blocks, as CSS clips. A wheel's gesture or a dragged thumb holds it back
  until the scroll comes to rest: a gesture rests at
  `hotty_host_end_gesture` or 150 ms after its last wheel, which
  `hotty_host_next_frame` counts as a deadline.

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

A touch that begins on the cells, outside every surface or where one lets
the pointer through, goes to `hotty_host_cells_touch`
(`Host::cells_touch`), every phase of it, with whether the program reads
the mouse and whether the terminal shows the screen rather than its
scrollback. The areas a program names with `a=touch` (SPEC §9.6) are kept
per screen, as its menu is: leaving the alternate screen
(`hotty_host_set_screen`) drops its areas, and a full reset
(`hotty_host_reset`) drops both. In an area, while the program reads the
mouse on the screen and without Alt, the touch is decided as on a surface:
the way the area drags makes it the program's mouse (2), and hotty-blitz
hands back the reports to make, in the program's mode and encoding: the
primary button pressed where the touch began, moved with the finger
wherever it goes, and released where it lifts, or on the last cell at a
cancel, with the keys held but one the terminal keeps a press for (Shift,
where Shift and a press select). The other way is a pan (0), and a tap
there is a press and a release where it lifted. Anywhere else the touch is
the terminal's (0).

## Text fields

A focused text field types and edits only as its program's keymap says
(SPEC §10.2): the default keymap, then each `data-keys` from the root to
the field. A bound key does its action, a printable key without `ctrl`,
`alt` or `super` types, and every other key goes to the program. Words, lines
and a password field are the spec's (`crates/hotty-blitz/src/edit.rs`);
moves by rows and pages follow the field's layout. A terminal offers a key
twice at most (SPEC §10.4), its keys named as myelin key names 0.1 names
them: first as the user pressed it, named from the key event with its
physical key and the character it types (`ctrl+key_w`, `ctrl+w`), before
its own shortcuts and translations (`hotty_host_key_pressed`), so
Command+Left is `super+arrow_left` and not the `ctrl+a` a macOS terminal
sends for it, and the physical name is looked up first; then, if the surface did not
use it, as the bytes its key encoding or a binding would write
(`hotty_host_key_bytes`), named by what the program would read
(`hotty::keys`), so a remapped key is the key it sends. The keys a
field does not use go on to the program as they came. Any other focused
element's `data-keys` gives keys to the program, or, in a document that
scrolls, scrolls with them (`scroll-down`, `scroll-end`, …), after the keys
the element uses itself; a field leaves those scroll actions out.

A program changes a control as the user would with `a=field` (SPEC §6.4),
only while the user has not changed it since: each control counts the
user's edits, undos, toggles, checks and picks as its `rev` (a radio
group, one name in one form, counts together), and `input` and `change`
carry it. Positions are bytes of the UTF-8 value, a textarea's CR LF
and lone CR made LF first, and a selection's ends snap to the start of a
grapheme cluster. An `a=field` sends nothing, is
one edit of the field's history and ends a run of typing; a field that
only `a=field` changed sends no `change` when focus leaves it. A field's
caret starts at the end of its value (`mods/blitz/caret-at-end.md`), and
it keeps its selection while not focused. A field with `data-on~=caret`
hears `caret` when the user moves its selection, at most one a frame (a
second in one frame waits for the next), and its `input` carries the
selection. This host has no input method yet, so a field never composes.

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

## Fonts

A host ships one font, AdwaitaMono Nerd Font Propo (Regular, Bold, Italic
and Bold Italic, in `crates/hotty-blitz/fonts/`), so a document looks the
same on every system and its Nerd Font icons draw without an installed font.
Italic is the font's own, as in hottyterm's cells, not Regular slanted. It
is the first
`monospace` font. In every other generic family it comes second: after the
system's choice for that family and before the fonts fontconfig lists after
it. So a symbol or icon the system's font lacks draws from it, at its own
width. The faces are decompressed on a thread of their own when a host
starts (about 85 ms).

The spec doesn't name the font: a document can't count on it on another host.

## Fidelity

[docs/fidelity/](docs/fidelity/) has the corpus rendered here next to
Chromium. The known gaps:
- forms (`type=password` shows its text);
- tables (captions, `colspan`);
- `text-overflow` on right-to-left lines (they clip);
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
- Blitz is MIT OR Apache-2.0, parley Apache-2.0 OR MIT, and vello_cpu
  Apache-2.0 OR MIT. Our patches on each, in `mods/`, are offered under that
  upstream's licence, the one its `mod.md` names (gov P-7).
- Stylo is MPL-2.0 and stays a dependency.
- The font in `crates/hotty-blitz/fonts/` is OFL-1.1 (`OFL.txt`). Its Nerd
  Fonts icon sets keep their own licences, listed with their texts in
  `NOTICE.txt`. A program that ships the library ships both files.

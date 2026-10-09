# The Blitz fork

The patches here are the whole fork: commits on upstream Blitz
`674d7d2144585baa0f7368b8ad76371c93fe1b65`, the revision `Cargo.toml` pins.
`Cargo.toml` swaps it in with `[patch."https://github.com/DioxusLabs/blitz"]`
path dependencies on `../blitz`. Like the Ghostty fork, it stays local: a
GitHub fork of a public repository would be public. Whether to offer these
changes upstream is an open question.

| patch | what |
| --- | --- |
| `0001-blitz-dom-*` | delta cost proportional to depth, not document size (below) |
| `0002-blitz-paint-*` | inline SVG follows its `preserveAspectRatio` |
| `0003-blitz-dom-blitz-paint-*` | a control in the disabled state acts disabled, attribute or not |
| `0004-blitz-paint-*` | outlines take `outline-offset` and every border style |
| `0005-blitz-dom-*` | a box whose layout changed gets its overflow and transform again (below) |
| `0006-blitz-dom-*` | an SVG's `<image>` loads a `data:` URL and no file (below) |
| `0007-blitz-traits-blitz-dom-*` | a request carries its destination: image, style, font, iframe, document (below) |
| `0008-blitz-dom-*` | an `<img>` shows the source `srcset`, `sizes` and `<picture>` choose (below) |
| `0009-blitz-dom-*` | an inline SVG's `<image>` draws what the document fetches for it (below) |
| `0010-blitz-dom-*` | an `<img>` chooses its source again when the device changes (below) |
| `0011-blitz-dom-blitz-paint-*` | form controls follow the used color scheme (below) |
| `0012-blitz-dom-*` | text controls are as wide as `size` and `cols`, as tall as `rows` (below) |
| `0013-blitz-dom-*` | a scroll container's client rect does not move with its own scroll (below) |
| `0014-blitz-dom-blitz-paint-*` | a document can lock scrolling along an axis (below) |
| `0015-blitz-dom-*` | a scroll container is hit only within its box, its content within its clip (below) |
| `0016-blitz-dom-*` | stylesheets of the user origin (below) |
| `0017-blitz-dom-*` | `:focus-visible` matches; a click shows the ring only on a text field (below) |
| `0018-blitz-dom-blitz-paint-*` | selected text takes the colours of its `::selection` (below) |
| `0019-blitz-dom-*` | Backspace edits a text input on macOS too (below) |
| `0020-blitz-dom-blitz-paint-*` | an embedder can hide the caret, to blink it (below) |
| `0021-blitz-dom-*` | a password input shows a mask and edits its value (below) |
| `0022-blitz-dom-*` | an embedder can edit a text input: its value, its selection, its rows (below) |
| `0023-blitz-dom-*` | a select has a selected option, submits it, and draws closed (below) |
| `0024-blitz-dom-*` | a table emptied of its rows forgets them (below) |
| `0025-blitz-dom-*` | a box gone `display: none` forgets the boxes built under it (below) |
| `0026-blitz-dom-*` | speculative relayout (0001) leaves nodes out of the layout tree alone (below) |
| `0027-blitz-dom-*` | a mutation's queued work skips the nodes it freed (below) |
| `0028-blitz-dom-*` | an inline svg is built again when its source changes: `currentColor` follows the colour (below) |

```
scripts/blitz-fork.sh        # clones Blitz to ../blitz, branch hotty, applies the patches
make check                   # `make` runs the script when ../blitz is missing
```

Removing the `[patch]` section goes back to upstream Blitz, which
hotty-blitz no longer builds against: it applies the network policy by
each request's destination (0007). With that one use undone, refusing every
fetch, everything still works, but deltas cost O(N) again, a stretched SVG
(`preserveAspectRatio="none"`) is drawn narrow and centred, the controls of
a detached surface can be clicked, toggled and typed into (silently: it
still reports nothing), and every outline is solid and just outside its
box, so one pulled inside a surface's root with a negative offset is not
drawn at all. A box resized only through its containing block keeps its
old overflow, and paint can cull it. An SVG can draw any image file on the
disk, and an `<img>` loads its `src` alone, never a `srcset` or a
`<picture>` source. An inline SVG's `<image>` draws no `cid:` resource
and nothing from the network. Form controls are light on a dark terminal,
and every text input is 300px wide, whatever its `size`. In a document
that scrolls, a scrolled box reports its rect moved by its own scroll, so
`area` and focus scroll it wrongly; a press above it lands on the items it
scrolled out of view; and along an axis the document did not ask for, a
fragment link, a thumb or a fling still moves it. And some deltas panic:
one that empties a table, one under a box that went `display: none`, or one
that inserts a form control and removes it again.

## Freed nodes

A delta frees the nodes it removes. Layout data, and a mutation's own queue,
outlived them and named them ("invalid SlotMap key used"), which in hottyterm
turned HOTTY off for the terminal. hotty-blitz's `fuzz` test found
them; `stale_nodes` has each, shrunk.

- **Tables (0024).** Construction returned before the table branch for a
  node with no children, so a table emptied of its rows kept its table
  context, and paint read the rows from it. The context now goes with the
  inline layout, first, and is built again while the node is a table with
  children.
- **Hidden boxes (0025).** A box that goes `display: none` is not
  constructed again (its parent is, and an inline parent no longer lists
  it), and its subtree, unstyled, reports no damage, not even a removal.
  It kept its layout children, anonymous blocks, inline layout, hoisted
  out-of-flow boxes and paint tree, as did the boxes under it, and Taffy
  (hidden layout, rounding) and paint still visit a hidden box. Damage
  propagation now has a hidden box forget all of it, for its former subtree
  too. Shown again, the subtree is constructed afresh. Blitz's own debug
  check, `assert_layout_parents_consistent`, failed after such a delta; it
  holds again.
- **Speculation (0026).** `speculate_layout` (0001) asked nodes that had
  just left the layout tree about their layout parent and laid out their
  children, both possibly freed. It now passes over a node unless each
  layout parent up to the root element lists it; its old container lays out
  again, as for any unsettled child.
- **A mutation's queue (0027).** The mutator queues work for its flush (a
  form control's form owner, a style element's sheet, a select's
  selectedness); a control inserted and freed by the same mutation stayed
  queued. The flush now skips nodes no longer there.

## Backspace on macOS

Upstream compiles the text input's Backspace arm out on macOS: there, Cocoa's
text system turns the key into a `deleteBackward:` standard keybinding
(`UiEvent::AppleStandardKeybinding`), and that does the deleting. A terminal
has no Cocoa text system and hotty-blitz sends no keybindings, so on macOS
Backspace did nothing in a text field (plexos HOTTY-BACKSPACE-01). With 0019
the arm applies everywhere. It is the only key the text input leaves to
Cocoa: Delete, the arrows, Home and End take the same path on every
platform.

## The caret blinks

Blitz paints a focused text input's caret solid. A browser blinks it, and
so does hotty-blitz (surface.rs): 0020 adds `BaseDocument::set_caret_shown`,
which the painter honours, and the surface turns it off and on on the
frame clock that plays animated images, repainting only the input.

## Password inputs

Upstream edits an `<input type=password>` as a text field, so it showed
the password. The fork keeps a password field's value beside the editor
(`TextInputData::password`), and the editor holds a bullet, U+2022 as in
Chrome, for each character, so the caret, selection and hit testing work
on what is shown. Each edit of the mask is made to the value too: every
edit replaces the selection, or deletes a run beside a collapsed one, with
what it inserts before the caret, so the selection before, the lengths
and the caret after give the range and the text. The input event, form
submission and `TextInputData::value` carry the value, and the `value`
attribute sets it. As in a browser, the value is not copied or cut (a
paste still types), the field composes nothing (an IME's commit still
types), and changing `type` to or from `password` masks the value or
shows it again, which is how a program makes a reveal toggle. hotty-blitz
has no clipboard, and its Ctrl and Cmd keys are the program's anyway.
blitz-dom's `text::tests` and hotty-blitz's
`a_password_field_shows_bullets_and_reports_its_value` check it.

## Text field keys

A text field's keys are its program's keymap (HOTTY SPEC §10.2), so
hotty-blitz does the editing actions itself rather than send keys Blitz's
editor would read its own way. 0022 gives it what it needs on
`BaseDocument`: `text_input_selection` reads a text input's value and
selection, `set_text_input` sets both (a password field's value too, its
mask following) and redraws, `move_text_input_rows` moves the caret by
laid-out rows from Parley's selection, and `text_input_rows_shown` counts
the rows the field shows, for a page.

## Selects

Blitz hid every `<option>` and kept no selectedness, so a select drew as
an empty box and never reached form data. 0023 keeps an option's
selectedness in its `ElementState::CHECKED` bit, as Gecko does, so
`option:checked` matches it, and the mutator settles it with HTML's
selectedness setting algorithm as options come, go, or have `selected`
set or removed. There is no script, so no dirtiness: the attribute, set
or removed, is the selectedness from then on. Form data has the selected
options. A select without `multiple` draws closed, as a browser draws
it: the selected option and an arrow, as wide as its widest option
(every option shares one grid cell, the others hidden). Its list is not
drawn: HOTTY lets a host show none (SPEC §10.2), and hotty-blitz picks
with the keys alone, through `BaseDocument::pick_option`. hotty-blitz's
`a_select_picks_with_keys_repaints_as_a_full_paint_and_submits_its_pick`
and the select vector check it. Still not drawn: a `multiple` select, or
one with `size` above 1, which a browser shows as a list box.

## Scrolling

HOTTY scrolls a document only along the axes it asks for (SPEC §5.1,
§5.3), and hotty-blitz decides itself what a wheel, a touch drag or a key
scrolls (`crates/hotty-blitz/src/scroll.rs`). Three patches make Blitz
agree with it:

- **0013:** `get_client_bounding_rect` is the border box, which a scroll
  container's own scroll offset does not move. The position walk
  subtracted it, so a scrolled box reported itself moved, and `area` and
  bringing an element into view went wrong inside it. `scroll_into_view`
  already compensated.
- **0014:** `BaseDocument::set_scroll_axes` locks an axis: every scroll
  target's range along it is empty, whoever scrolls (the user, a fragment
  link, a fling, a thumb), locking puts the offsets back to zero, and no
  scrollbar is drawn or hit there. A text input still scrolls its own text
  to follow the caret.
- **0015:** hit testing added a node's scroll offset to the point before
  testing it against the node's box, so a box scrolled down took points
  above itself and handed them to the children it had scrolled out of
  view. The point is tested against the box first; the scroll offset moves
  only the content, and content a box clips is not hit outside its padding
  box, where it is not drawn.

Blitz draws an element's overlay scrollbars (the `scrollbars` feature,
which hotty-blitz turns on) but not the viewport's; hotty-blitz draws the
root's itself, with the same look and fade.

## Form controls in the color scheme

Upstream's UA stylesheet makes fields white and buttons light grey, and
paints checkboxes and radio buttons white with the text color as their
accent, so a surface with the host stylesheet's `color-scheme: dark`
(HOTTY SPEC §8) showed light controls on a dark terminal. Browsers take
these colors from the CSS system colors, which resolve in the element's
used color scheme, and Stylo already resolves them that way. The fork's
UA stylesheet gives fields `Field` and `FieldText`, and buttons
`ButtonFace` and `ButtonText`. Their borders are Chromium's in each
scheme (`light-dark()`). Checkboxes and radio buttons are drawn whole in
the element's used color scheme (`BaseDocument::used_color_scheme`), with
Chromium's colors for `accent-color: auto` (Stylo's Servo build has no
`accent-color`) and its 13px geometry. A document's own colors still win.
The shades come from Stylo's system colors, so a dark field is #2D2D2D
where Chromium's is #3B3B3B, and a light button #DCDCDC where it is
#EFEFEF. hotty-blitz's `form_controls_follow_the_used_color_scheme` and
`a_new_color_scheme_repaints_controls_like_a_fresh_document` check it.

## The host stylesheet's palette

HOTTY SPEC §8 dresses form controls, focus, links and selected text in
the terminal's 16 colours, with rules that must win over the engine's
defaults and lose to any rule of the document's: a browser host puts
them in a cascade layer of the author origin. Three patches let Blitz
show them.

- **User stylesheets** (`DocumentConfig::user_stylesheets`,
  `add_user_stylesheet`). The host stylesheet was a second user-agent
  sheet, where Blitz's `input { background-color: Field }` outranked
  §8's zero-specificity `:where(input, …)`. As a sheet of the user
  origin it wins over the user agent's whatever the specificity, and
  every rule of the document's wins over it.
- **`:focus-visible`** matched nothing. It matches the focus ring
  (`ElementState::FOCUSRING`), which focus from the keyboard or a script
  sets. Focus moved by the pointer (`set_focus_by_pointer`, which
  Blitz's own click handling now calls) keeps it only on a text field,
  as browsers do.
- **`::selection`**. Selections were a fixed light blue under text in
  its own colours. The highlight is now the inline root's (or the text
  field's) `::selection` background, or that blue when it sets none, and
  the selected text is stroked again over it, clipped to it, in each
  run's `::selection` colour.

What §8 asks that Blitz still cannot show: `accent-color` (Stylo's
Servo build drops it, so checks keep the scheme's colours, above), and
`::placeholder`, since Blitz draws no placeholder text at all.
hotty-blitz's `tests/palette.rs` checks the computed styles in a dark
and a light scheme, and the selection's pixels.

## Text control sizes

Upstream makes a text input 300px wide, or the available width when
that is less, whatever its `size`, and a textarea 300px wide unless it
has `cols`. The fork counts characters as Chromium does:
- one character is the primary font's average width (OS/2
  `xAvgCharWidth`), or the advance of `0`;
- an input is `size` characters wide (20 by default), plus the font's
  widest glyph less one character;
- a textarea is `cols` characters wide (20 by default), with no scrollbar
  gutter, because Blitz's scrollbars are overlays;
- a row (`rows`, 2 by default) is the line height, which for
  `line-height: normal` is the font's own, as Parley lays the text out,
  rather than 1.2em.

For Noto Sans and Noto Sans Mono at 16px, every width matches Chromium's
to the pixel. hotty-blitz's
`text_controls_are_as_wide_as_size_and_cols_and_as_tall_as_rows` checks
it.

## SVG images load no files

Blitz parses every SVG, an `<img>` or CSS image and inline `<svg>` alike,
with usvg's default options, whose image resolver reads any `<image href>`
that is not a `data:` URL as a path on the disk: a page could draw any
image the user's files hold, with no `NetProvider` asked (HOTTY SPEC §12).
The fork's resolver loads `data:` URLs, and what the document fetched for
an inline SVG's `<image>` (below), and nothing else.
`an_image_loads_a_data_url_and_no_file` in blitz-dom checks it, and
hotty-blitz's `an_svg_draws_no_image_file` and
`files_documents_and_other_schemes_are_never_fetched`.

## Images in inline SVG

usvg resolves an `<image>`'s href as it parses, synchronously, and Blitz
parses an inline `<svg>` while it constructs boxes, so nothing could be
fetched for it. The fork fetches an SVG `<image>` as it does an `<img>`:
when it joins the document or its `href` (or `xlink:href`) changes,
through the `NetProvider`, as an `Image` request. The bytes are kept by
resolved URL (`svg_image.rs`); when they arrive, the outermost `<svg>` is
constructed, and so parsed, again, and the resolver hands usvg those bytes
by href as written. `reload_resource_by_href` fetches such an image again.
An SVG that is itself an image (an `<img>`, a CSS image) still loads
nothing but `data:`, as in a browser. This is how `cid:` works in an SVG
`href` (HOTTY SPEC §7.1), and how `img-src` reaches images in SVG (§7.2).
hotty-blitz's `an_inline_svgs_image_resolves_cid_resources` and
`an_inline_svgs_images_are_images` check it.

## Inline SVG follows its colour

Blitz draws an inline `<svg>` from one usvg parse of its markup, made
while boxes are constructed, with `currentColor` resolved to each
element's colour then (`Node::outer_html`). A colour change later is
style damage only, so an icon drawn with `fill="currentColor"` kept its
first colour through a class on its parent, a theme's style text,
`:hover` and `:focus`. The A2UI kit draws its icons that way. The fork
keeps a hash of the source each inline svg was built from
(`inline_svg_sources`). When an svg or anything under it is damaged and
its source differs, `propagate_damage_flags` constructs that svg again,
and only that one: its ancestors get `CONSTRUCT_DESCENDENT`, as for a
reorder. hotty-blitz's `tests/svg.rs` checks the pixels after each kind
of change.

## Request destinations

A `NetProvider` got a URL and nothing about what it was for, so it could
not apply a policy per kind of resource, as CSP's `img-src`, `style-src`
and `font-src` do (HOTTY SPEC §7.2). `Request` now has fetch's
`destination`, and every request a document makes sets it: `Image` for
`<img>` and CSS images, `Style` for stylesheets, their reload and
`@import`, `Font` for `@font-face`, `Iframe` for an `<iframe>`'s document,
and `Document` for a navigation. `Request::get` leaves it `Empty`. The
struct was `non_exhaustive` already. hotty-blitz's `tests/network.rs`
checks each directive.

## srcset and picture

Upstream loads an `<img>`'s `src` and nothing else. `BaseDocument::image_source`
selects an image source as HTML does: the first `<source>` of its
`<picture>` whose `type` this build decodes and whose `media` matches (a
stylo `MediaList` against the stylist's device), else its own `srcset`,
else its `src`. Of a `srcset`, the candidate with the smallest density at
or above the device's, else the densest; a `w` candidate's density is its
width over the size `sizes` gives it (stylo's `SourceSizeList`). Changing
the attributes loads the image again, and so does a new device (0010):
when the viewport's size, scale or colour scheme, or the media type,
changes, every `<img>` whose source the device chose chooses again and
loads what it chose if that differs; the old image shows until the new
one arrives, and one chosen for an old device that arrives late is not
shown. hotty-blitz's `srcset_resolves_cid_resources`,
`srcset_and_picture_choose_what_is_fetched` and
`srcset_and_picture_choose_again_for_a_new_scale_or_theme` check it, and
anim.rs finds a playing image by the same source.

## Disabled controls

A detached surface's form controls act as if each had the `disabled`
attribute (HOTTY SPEC §5.5), but the document keeps the program's
attributes. hotty-blitz puts them in the disabled state instead
(`Node::disable`), which `:disabled` already matches. Upstream's pointer
handling and form-control painting ask for the attribute; the fork has them
ask `ElementData::is_disabled`, the attribute or the state. hotty-blitz's
`a_detached_surfaces_controls_are_disabled` and
`a_detached_document_paints_as_if_its_controls_had_disabled` check it.

## SVG: preserveAspectRatio

Upstream paints every inline `<svg>` as `object-fit: contain`. A browser
fits the viewBox to the element's box as its `preserveAspectRatio` says
(SVG 2, 8.6): `none` stretches it, `slice` covers the box, and the default
fits inside it. Charts drawn in a fixed viewBox and stretched to their box
need `none`. hotty-blitz's `inline_svg_follows_its_preserve_aspect_ratio`
checks it.

## Outlines

Upstream draws every outline solid, just outside the border box: it ignores
`outline-offset`, and draws a dashed or dotted outline solid. A surface's
root fills the surface, so an outline on it must come inside with a
negative offset, and upstream drew it off the surface (hotty-demo's
"its surfaces" view: a dashed outline on each surface). The fork draws an
outline as the border of a box of its own, offset from the border box,
with the border's edge code, so it takes every border style. It also paints
the outline after the element's content, as CSS 2.1 Appendix E does, so the
element's background does not cover one inside it. hotty-blitz's
`an_outline_takes_its_offset_and_style` checks it. Dotted outlines have
round dots, as Blitz's dotted borders do; Chromium's are square.

## Overflow after a relayout

Blitz computes a node's transform and scrollable overflow again only when
its own style damage asks for it. Damage flows up the tree, so a box whose
size changed because an ancestor changed kept its old overflow. Paint
culls a box by its overflow: a delta to a custom property that sets the
height and the `translateY` of an absolutely positioned ring left its
stretched child with the overflow of the old height, which the new
transform moved off the surface. Nothing of the child was painted, on any
later paint either. The fork records every node whose final layout
changed (`set_final_layout`), with the boxes that reach it in
`resolve_transforms`, and computes those again too. hotty-blitz's
`a_var_delta_lays_out_like_a_fresh_document` checks it.

## Delta cost: what it changes

Upstream, one changed cell costs every phase of `resolve` a walk of the whole
document. The fork makes each phase stop where the change stops:

| phase | upstream | fork |
| --- | --- | --- |
| construct (`resolve.rs`) | walks every node | skips elements with no damage below them |
| layout cache (`layout/cache.rs`) | taffy's 9 entries per node | grows per node, same matching rules |
| layout (`layout/speculate.rs`) | every ancestor lays out all its children again | a flex or grid container keeps its cache when its changed items give the same answers |
| rounding (`layout/round.rs`) | `taffy::round_layout` over every node | only changed nodes, and the subtrees of moved ones |

**Cache.** A content-sized container k levels deep is measured under about
3 + 5k distinct constraints per layout pass, one set per ancestor's intrinsic
sizing pass. Past the second level, taffy's 9 entries evicted each other in a
cycle. Every unchanged sibling of the changed path then missed, and the misses
cascaded down its subtree. `LayoutCache` keeps up to 64 entries, and a node
measured once allocates nothing. It also keeps each request's full input,
which the next step needs.

**Speculation.** A flex or grid item is an independent formatting context: its
container sees it only through what it answers to the inputs the container
passes. After construction, each changed item is asked again everything its
container asked it last time. When every answer matches, the container keeps
its old cache, and the same test applies one level up. A container qualifies
when:

- it is a flex or grid container;
- its own content and style did not need relayout;
- construction left its layout children as they were;
- every item whose cache was cleared gave the same answers.

An answer that differs only in the overflow of an item that clips or contains
its overflow still counts: taffy ignores that overflow for the container, and
the item's own layout is updated in place. A changed out-of-flow box turns
speculation off for that resolve.

**Rounding.** `set_unrounded_layout` records the nodes whose layout changed.
Inline layout writes its boxes directly, so an inline root that was laid out
is recorded whole. Only recorded nodes, and the subtrees of recorded nodes that
moved, are rounded again. A resolve that changed more than a quarter of the
tree rounds everything, as upstream does.

## Checks

- Debug builds compare every incremental rounding with `taffy::round_layout`
  and panic on a difference.
- hotty-blitz's `incremental_layout_matches_a_fresh_document` changes a page
  with deltas for 22 steps and compares every frame, pixel for pixel, with
  the same page built from scratch. The page mixes nested content-sized
  flex-wrap, fractional sizes, grid, a float, a table, absolute boxes,
  baseline alignment, a flex column and fixed-size clipped cells.
  - Forcing every item to count as settled fails at step 1 (3,194 pixels).
  - Skipping the rounding of moved subtrees trips the debug check.
- Blitz's own suites pass on the fork, in debug with the rounding check on:
  `blitz-tests` and `blitz-dom`, 299 tests.

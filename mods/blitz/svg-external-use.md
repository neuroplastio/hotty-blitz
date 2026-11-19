---
title: "A <use> draws an element of another SVG resource"
patch: 0104-blitz-dom-a-use-draws-an-element-of-another-SVG-reso.patch
upstream-fit: maybe
---

**What it changes:** `<use href="sprite.svg#icon">` in an inline SVG.
Its resource is fetched through the document's `NetProvider`, as an
inline SVG's `<image>` is (0030, 0032), but with the empty destination,
not as an image, and kept by URL. When the `<svg>` is constructed, the
element `icon` names is copied into its `<defs>`, with everything that
element refers to in its resource (`href="#…"`, `url(#…)`), under an id
prefix that is nowhere in the `<svg>`. The `<use>` then refers to the
copy, so it is drawn in the context of the `<use>`, `currentColor`
included, as a same-document `<use>` is (0070). The resource is parsed
once, the first time a `<use>` draws from it, and each element's copy is
written once. A resource that arrives late, is sent again, or a changed
href constructs the `<svg>` again. An href with no fragment fetches
nothing. A sprite's own `<image>`, or a `<use>` of a third resource, is
copied but not drawn.

**Why we carry it:** SPEC §7.1 has a host resolve `cid:` in an SVG
`href`, and a sprite sent once as a resource is the way a hotty program
shares icons between documents, coloured by the text they sit in.
Blitz resolved `cid:` only for `<image>`. §7.2 has no directive for
`<use>`, so hotty-blitz fetches nothing for the empty destination from
the network: only `cid:` sprites are drawn.

**For upstream:** maybe. Browsers draw external `<use>` references to
same-origin resources, and Blitz has the fetch path for `<image>`
already. Copying the element into the `<svg>` is a workaround for usvg,
which resolves only references within the document it parses; CSS in
the sprite's `<style>` that selects by id does not follow the renamed
ids. A cleaner fix would be usvg taking a resolver for external
references.

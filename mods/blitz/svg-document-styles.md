---
title: "An inline svg is drawn with the styles the document gives it"
patch: 0042-blitz-dom-an-inline-svg-is-drawn-with-the-styles-the.patch
upstream-fit: yes
---

**What it changes:** Blitz draws an inline `<svg>` from one usvg parse of
its markup, and usvg reads the markup alone: no rule of the document's
stylesheets, no custom property. A shape filled by `.icon rect { fill: … }`,
or by `var()` in a rule or in its own `style`, drew black. The markup usvg
is given now carries, in each element's `style`, the painting properties
the cascade computed for it (fill, stroke and their opacities, widths,
caps, joins, dashes, and a nested element's opacity), as values usvg
parses: colours as `rgb()`, a paint server as `url(#id)`. A property is
written where the element sets it or where it differs from its parent's,
so a `<use>` still passes its values to a symbol's shapes that set none.
For that cascade to be the browser's, an SVG element's presentation
attributes (`fill="…"` and the others Stylo computes) are now its
presentational hints, below every author rule, as in a browser; before,
Stylo didn't see them. The svg's own `opacity` is its box's, which Blitz
paints, so usvg is told not to apply it again. The source compared to
rebuild an svg (`inline-svg-rebuild.md`) is this markup, so a theme that
changes a custom property redraws its icons.

**Why we carry it:** hotty programs colour icons and charts with their
theme's custom properties; each drew black.

**For upstream:** yes. Both halves are browser behaviour Blitz lacks:
presentation attributes as presentational hints, and the document's
styles in inline SVG. Upstream may prefer to hand usvg a tree rather than
markup, which this does not attempt.

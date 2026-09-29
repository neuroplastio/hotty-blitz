# Fidelity: hotty-blitz (Blitz) next to Chromium

`make oracle` renders every page of the HOTTY corpus twice, at 80 columns of 20×42 px
cells (scale 2), with the same host stylesheet (`hotty css`): once by
`hotty render`, once by headless Chromium 153 (Playwright). Left is hotty-blitz,
right is Chromium. Read on 2026-09-29, Blitz `674d7d2`.

| page | tests |
| --- | --- |
| [01-card](01-card.png) | flow text, flexbox, `color-mix()`, `rlh` |
| [02-typography](02-typography.png) | headings, inline formatting, lists, blockquote, `pre` |
| [03-grid](03-grid.png) | grid areas, `fr`, `minmax`, `auto-fill`, spans |
| [04-table](04-table.png) | tables, `colspan`, `border-collapse`, caption |
| [05-forms](05-forms.png) | every common form control |
| [06-effects](06-effects.png) | shadows, gradients, transforms, opacity, clip-path, filter |
| [07-unicode](07-unicode.png) | CJK, emoji, RTL, combining marks |
| [08-selectors](08-selectors.png) | `:has()`, `:is()`, nesting, `@layer`, `clamp()`, container queries |
| [09-overflow](09-overflow.png) | overflow, sticky, `text-overflow`, absolute positioning |
| [10-svg-images](10-svg-images.png) | inline SVG, `data:` images |

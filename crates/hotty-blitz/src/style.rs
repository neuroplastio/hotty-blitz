//! The host's stylesheet (SPEC §8): the terminal's look, handed to every
//! document as a stylesheet of the user origin, so that it wins over
//! Blitz's own defaults and any rule of the document's wins over it. What
//! scrolls is each document's own ([`scroll_css`]).

use crate::{Config, Rgb};
use std::fmt::Write;

pub fn host_css(config: &Config) -> String {
    let m = &config.metrics;
    let t = &config.theme;
    let cell_w = m.cell_w as f32 / m.scale;
    let cell_h = m.cell_h as f32 / m.scale;
    let font_size = config.font_size.unwrap_or((cell_h / 1.25).max(6.0));
    let mut css = String::new();
    let _ = writeln!(css, ":root {{");
    let _ = writeln!(
        css,
        "  color-scheme: {};",
        if t.dark { "dark" } else { "light" }
    );
    let _ = writeln!(css, "  --hotty-fg: {};", hex(t.fg));
    let _ = writeln!(css, "  --hotty-bg: {};", hex(t.bg));
    for (i, c) in t.palette.iter().enumerate() {
        let _ = writeln!(css, "  --hotty-ansi-{i}: {};", hex(*c));
    }
    // The terminal's blue: plain blue is hard to read on a dark background.
    let _ = writeln!(
        css,
        "  --hotty-accent: var(--hotty-ansi-{});",
        if t.dark { 12 } else { 4 }
    );
    let _ = writeln!(css, "  --hotty-cell-w: {cell_w}px;");
    let _ = writeln!(css, "  --hotty-cell-h: {cell_h}px;");
    let _ = writeln!(css, "  --hotty-font: {};", font_stack(&config.font_family));
    let _ = writeln!(css, "  font-family: var(--hotty-font);");
    let _ = writeln!(css, "  font-size: {font_size}px;");
    // An explicit line-height, so that `1rlh` is one terminal row (`rlh`
    // is 0 while the root's line-height is `normal`).
    let _ = writeln!(css, "  line-height: {cell_h}px;");
    let _ = writeln!(css, "  color: var(--hotty-fg);");
    let _ = writeln!(css, "  background: var(--hotty-bg);");
    // Stylo, as Servo builds it, does not know accent-color yet and drops
    // it: checks keep Blitz's colours.
    let _ = writeln!(css, "  accent-color: var(--hotty-accent);");
    let _ = writeln!(css, "}}");
    css.push_str("body { margin: 0; }\n");
    css.push_str(CONTROLS);
    css
}

/// Controls in the terminal's colours, not the engine's: ansi-8 is the dim
/// grey of borders, buttons, placeholders and what is disabled. :where()
/// keeps the base rule below the state rules after it, such as :disabled.
const CONTROLS: &str = "\
:where(input:not([type=checkbox], [type=radio], [type=range]), textarea,
       select, button) {
  color: var(--hotty-fg);
  background: var(--hotty-bg);
  border: 1px solid var(--hotty-ansi-8);
}
button:enabled, input:is([type=button], [type=submit], [type=reset]):enabled {
  background: var(--hotty-ansi-8);
}
::placeholder, :disabled { color: var(--hotty-ansi-8); }
:focus-visible { outline: 1px solid var(--hotty-accent); }
:any-link { color: var(--hotty-accent); }
::selection { color: var(--hotty-bg); background: var(--hotty-accent); }
";

/// What a document scrolls (SPEC §5.3), a user-agent sheet of its own:
/// the root clips along the axes it did not ask for (`axes`, 1 vertically,
/// 2 horizontally), as the host stylesheet's `overflow: hidden` (§8) does
/// for one that asked for none. Blitz keeps those axes still whatever the
/// document's CSS says (`set_scroll_axes`).
pub fn scroll_css(axes: u8) -> &'static str {
    match axes & 3 {
        0 => ":root { overflow: hidden; }\n",
        1 => ":root { overflow-x: hidden; }\n",
        2 => ":root { overflow-y: hidden; }\n",
        _ => "",
    }
}

fn font_stack(family: &str) -> String {
    if family.is_empty() {
        "monospace".to_string()
    } else {
        format!("\"{}\", monospace", family.replace('"', ""))
    }
}

pub fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

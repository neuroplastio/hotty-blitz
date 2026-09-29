//! The host's stylesheet (SPEC §8): the terminal's look, handed to every
//! document as a user-agent sheet after Blitz's own defaults.

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
    let _ = writeln!(css, "}}");
    css.push_str("body { margin: 0; }\n");
    css
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

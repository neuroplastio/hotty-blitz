// Renders corpus pages in headless Chromium (Playwright) with the same host
// stylesheet and pixel size as `hotty render`, for side-by-side comparison.
//   NODE_PATH=~/.cache/hotty/oracle/node_modules node scripts/oracle.cjs <css-file> <out-dir> <page.html:height>...
//
// Three things make Chromium draw as the host does (hotty-ui's
// scripts/gate/gate.cjs found each):
// - The host stylesheet goes in a cascade layer below every author rule,
//   where SPEC §8 puts it and where Blitz's user origin is for the cascade.
//   As an unlayered author sheet, `button:enabled` would beat a page's
//   `.btn` in Chromium alone.
// - The host's font is the one hotty-blitz ships, the first `monospace` in
//   Blitz (crates/hotty-blitz/src/fonts.rs), where Chromium's `monospace` is
//   the system's. So the host stylesheet names it (`hotty css --font`, as
//   for a terminal that names its font, which draws the same in Blitz), and
//   Chromium loads its faces from crates/hotty-blitz/fonts. A web font
//   can't be the generic family, so a page that asks for `monospace` itself
//   still gets the system's in Chromium.
// - Text is unhinted, as Blitz draws it: hinted, Chromium rounds a font's
//   advances to whole pixels, and a line drifts a pixel every few words.
const fs = require("fs");
const path = require("path");
const { chromium } = require("playwright");

const FONT = "AdwaitaMono Nerd Font Propo"; // fonts.rs's FAMILY
const FONTS = path.resolve(__dirname, "../crates/hotty-blitz/fonts");
const ORIGIN = "https://oracle.invalid"; // served by this script alone

(async () => {
  const [cssFile, outDir, ...pages] = process.argv.slice(2);
  const css = fs.readFileSync(cssFile, "utf8");
  const served = new Map(); // path → [type, body]
  const faces = [["Regular", 400, "normal"], ["Bold", 700, "normal"], ["Italic", 400, "italic"], ["BoldItalic", 700, "italic"]];
  const fontCSS = faces
    .map(([f, w, st]) => {
      const file = `AdwaitaMonoNerdFontPropo-${f}.woff2`;
      served.set(`/fonts/${file}`, ["font/woff2", fs.readFileSync(path.join(FONTS, file))]);
      return `@font-face { font-family: "${FONT}"; src: url(/fonts/${file}) format("woff2"); font-weight: ${w}; font-style: ${st}; }`;
    })
    .join("\n");
  const host = `<style>${fontCSS}\n@layer hotty-host {\n${css}\n}</style>`;

  const browser = await chromium.launch({ args: ["--font-render-hinting=none"] });
  const ctx = await browser.newContext({ viewport: { width: 800, height: 600 }, deviceScaleFactor: 2 });
  await ctx.route(`${ORIGIN}/**`, (route) => {
    const hit = served.get(new URL(route.request().url()).pathname);
    return hit ? route.fulfill({ status: 200, contentType: hit[0], body: hit[1] }) : route.fulfill({ status: 404 });
  });
  const page = await ctx.newPage();
  for (const spec of pages) {
    const [file, h] = spec.split(":");
    const name = path.basename(file, ".html");
    const html = fs.readFileSync(file, "utf8").replace(/^\s*<!doctype[^>]*>/i, "");
    const doc = html.includes("<head>") ? html.replace("<head>", "<head>" + host) : host + html;
    served.set(`/${name}.html`, ["text/html; charset=utf-8", `<!doctype html>${doc}`]);
    const height = Math.round(Number(h) / 2);
    await page.setViewportSize({ width: 800, height });
    await page.goto(`${ORIGIN}/${name}.html`, { waitUntil: "load" });
    await page.evaluate(() => document.fonts.ready);
    const out = path.join(outDir, name + ".chromium.png");
    await page.screenshot({ path: out, clip: { x: 0, y: 0, width: 800, height } });
  }
  await browser.close();
})();

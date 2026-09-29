// Renders corpus pages in headless Chromium (Playwright) with the same host
// stylesheet and pixel size as `hotty render`, for side-by-side comparison.
//   NODE_PATH=~/.cache/hotty/oracle/node_modules node scripts/oracle.cjs <css-file> <out-dir> <page.html:height>...
const fs = require("fs");
const path = require("path");
const { chromium } = require("playwright");

(async () => {
  const [cssFile, outDir, ...pages] = process.argv.slice(2);
  const css = fs.readFileSync(cssFile, "utf8");
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 800, height: 600 }, deviceScaleFactor: 2 });
  for (const spec of pages) {
    const [file, h] = spec.split(":");
    const html = fs.readFileSync(file, "utf8");
    const height = Math.round(Number(h) / 2);
    await page.setViewportSize({ width: 800, height });
    await page.setContent(`<!doctype html><style>${css}</style>${html}`, { waitUntil: "load" });
    const out = path.join(outDir, path.basename(file, ".html") + ".chromium.png");
    await page.screenshot({ path: out, clip: { x: 0, y: 0, width: 800, height } });
  }
  await browser.close();
})();

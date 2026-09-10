// Perplexity autocomplete, read the only way it can be: from a browser.
//
// Perplexity has no public suggest endpoint. The REST route answers 403
// behind Cloudflare to anything that is not a browser, and the real
// suggestions arrive over a WebSocket the page opens after the Cloudflare
// check. So this drives a headless Chromium: open the page once, type each
// probe into the search box, and read the WebSocket frames, which carry
// `[prefix, [suggestion, ...], seq, [{text}, ...]]`.
//
// Usage: node perplexity-suggest.mjs <probes.json>   (a JSON array of strings)
// Output: JSON object { "<probe>": ["suggestion", ...], ... } on stdout.
// Diagnostics go to stderr. Exit 0 even when some probes yield nothing;
// exit 1 only when the page itself could not be reached.
//
// Fragile by nature: a Perplexity front-end change or a Cloudflare policy
// change breaks it. That is the price of the source; keywordtool.io pays it
// too.

import { chromium } from "playwright";
import { readFileSync } from "node:fs";

const probes = JSON.parse(readFileSync(process.argv[2], "utf8"));
const out = {};
const log = (...a) => console.error("[pplx]", ...a);

const browser = await chromium.launch({ headless: true });
const ctx = await browser.newContext({
  locale: process.env.PPLX_LOCALE || "en-US",
  userAgent:
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36",
});
const page = await ctx.newPage();

// Every frame that looks like a suggestion list, keyed by its prefix. The
// page fires one per keystroke; we keep the last for each full probe.
const frames = new Map();
page.on("websocket", (ws) => {
  ws.on("framereceived", (f) => {
    try {
      const s = String(f.payload);
      if (!s.startsWith("[")) return;
      const arr = JSON.parse(s);
      if (Array.isArray(arr) && typeof arr[0] === "string" && Array.isArray(arr[1])) {
        frames.set(arr[0].toLowerCase(), arr[1].filter((x) => typeof x === "string"));
      }
    } catch {}
  });
});

try {
  await page.goto("https://www.perplexity.ai/", { waitUntil: "domcontentloaded", timeout: 45000 });
} catch (e) {
  log("cannot open perplexity.ai:", e.message);
  await browser.close();
  process.exit(1);
}
await page.waitForTimeout(3000);
const box = await page.$('textarea, [contenteditable="true"], input[type=text]');
if (!box) {
  log("no search box found; front-end changed?");
  await browser.close();
  process.exit(1);
}

let hits = 0;
for (const probe of probes) {
  await box.click();
  await page.keyboard.press("Meta+A");
  await page.keyboard.press("Backspace");
  // Type the whole probe at once, then nudge with a trailing space and
  // backspace so a frame for the exact probe is requested.
  await page.keyboard.type(probe, { delay: 0 });
  await page.waitForTimeout(350);
  await page.keyboard.type(" ", { delay: 0 });
  await page.keyboard.press("Backspace");
  await page.waitForTimeout(450);
  const got = frames.get(probe.toLowerCase()) || [];
  out[probe] = got;
  if (got.length) hits++;
}
log(`${probes.length} probes, ${hits} with suggestions`);
await browser.close();
process.stdout.write(JSON.stringify(out));

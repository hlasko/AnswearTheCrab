// Real browser acceptance for the compiled application. No provider requests.
// Run only against a disposable atp_chart_* database and its isolated server.
import { chromium, webkit } from 'playwright';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile } from 'node:fs/promises';

const base = process.env.ATP_CHART_BASE_URL || 'http://127.0.0.1:3018';
const db = process.env.ATP_CHART_TEST_DATABASE_URL;
if (process.env.ATP_SMOKE_ALLOW_FIXTURES !== '1' || !db || !/^postgresql:\/\/\/atp_chart_[a-z0-9_]+$/.test(db)) {
  throw new Error('Use ATP_SMOKE_ALLOW_FIXTURES=1 and a disposable postgresql:///atp_chart_* database.');
}
const sql = text => execFileSync('psql', [db, '-X', '-v', 'ON_ERROR_STOP=1', '-c', text], { encoding: 'utf8' });
const done = '44444444-4444-4444-8444-444444444444';
const pending = '55555555-5555-4555-8555-555555555555';
const empty = '66666666-6666-4666-8666-666666666666';
sql(`
  insert into searches(id, keyword, language, country, source, status, suggestion_count) values
  ('${done}', 'Chart test coffee', 'en', 'us', 'google', 'done', 144),
  ('${pending}', 'Chart test polling', 'en', 'us', 'google', 'pending', 0),
  ('${empty}', 'Chart test empty', 'en', 'us', 'google', 'done', 0)
  on conflict(id) do update set status=excluded.status;
  delete from suggestions where search_id in ('${done}', '${pending}', '${empty}');
  insert into suggestions(search_id,text,category,modifier,search_volume,cpc)
  select '${done}', 'coffee question ' || g || ' phrase ' || n, 'questions', 'group ' || g, n*100, 1.25
  from generate_series(1,12) g cross join generate_series(1,12) n
  on conflict do nothing;
`);
const engine = process.env.ATP_CHART_BROWSER === 'webkit' ? webkit : chromium;
const browser = await engine.launch({ headless: true });
const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
const page = await context.newPage();
page.setDefaultTimeout(15000);
const errors = [];
const httpChecks = [];
page.on('pageerror', e => { errors.push(e.message); console.error('BROWSER ERROR', e.stack); });
page.on('console', message => {
  if (message.type() === 'error' && message.text().includes('panicked')) console.error('BROWSER PANIC', message.text());
});
page.on('response', response => {
  if (response.status() >= 400) httpChecks.push(response.text().then(body => {
    // The app auto-requests optional AI summaries. Paid providers are disabled
    // in this installation. No other failed request is accepted.
    assert.match(response.url(), /\/api\/summarise_section/);
    assert.equal(response.status(), 500);
    assert.equal(body, 'ServerError|summaries need OPENROUTER_API_KEY');
  }));
});
const ready = async () => {
  await page.waitForFunction(() => {
    const stages = [...document.querySelectorAll('.wheel-stage')];
    return stages.length > 0 && stages.every(s => s.getAttribute('aria-busy') === 'false'
      && s.querySelector('.chart-ready') && s.querySelector('.chart-loading').hidden
      && s.querySelector('.chart-error').hidden);
  });
  const charts = await page.locator('.wheel-svg').evaluateAll(elements => elements.map(svg => ({
    namespaces: [...new Set([svg, ...svg.querySelectorAll('*')].map(n => n.namespaceURI))],
    spokes: svg.querySelectorAll('.spoke').length,
    painted: [...svg.querySelectorAll('.spoke')].every(n => n.getBBox().width > 0 && n.getBBox().height > 0),
    visible: getComputedStyle(svg).visibility,
  })));
  assert.ok(charts.every(c => c.spokes > 0 && c.painted && c.visible === 'visible'));
  assert.ok(charts.every(c => c.namespaces.length === 1 && c.namespaces[0] === 'http://www.w3.org/2000/svg'));
  assert.equal(await page.locator('.wheel .png:disabled').count(), 0);
  return charts;
};
try {
  // Slow initial hydration: spinner is real, chart hidden, export disabled.
  let release;
  const gate = new Promise(resolve => { release = resolve; });
  await page.route('**/*.wasm*', async route => { await gate; await route.continue(); });
  await page.goto(`${base}/search/${done}`, { waitUntil: 'domcontentloaded' });
  await page.locator('.wheel-stage .chart-loading').first().waitFor({ state: 'visible' });
  assert.equal(await page.locator('.wheel-content').first().evaluate(e => getComputedStyle(e).visibility), 'hidden');
  assert.ok(await page.locator('.wheel .png').first().isDisabled());
  assert.equal(await page.locator('.wheel-stage').first().getAttribute('aria-busy'), 'true');
  release();
  await ready();
  await page.unrouteAll({ behavior: 'wait' });
  console.log('PASS cold SSR: loading -> hydrated SVG -> enabled PNG');

  // Navigate via the actual router, retaining a window marker to exclude reload.
  await page.getByRole('link', { name: 'Research', exact: true }).click();
  const link = page.locator(`.recent-list a[href="/search/${done}"]`).first();
  await link.waitFor();
  await page.evaluate(() => { window.chartNavigationMarker = 'retained'; });
  await link.click();
  const spaCharts = await ready();
  assert.equal(await page.evaluate(() => window.chartNavigationMarker), 'retained');
  console.log('PASS first SPA navigation: SVG-only nodes with nonzero graphics bounds', JSON.stringify(spaCharts));

  const downloadEvent = page.waitForEvent('download');
  await page.locator('.wheel .png').first().click();
  const download = await downloadEvent;
  const bytes = await readFile(await download.path());
  assert.equal(bytes.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
  assert.ok(bytes.length > 1000);
  console.log('PASS first-navigation PNG export', bytes.length, 'bytes');

  // Failure is explicit and retry actually remounts, rather than spinning forever.
  await page.getByRole('link', { name: 'Research', exact: true }).click();
  await link.waitFor();
  await page.evaluate(() => {
    const observer = new MutationObserver(() => {
      const svg = document.querySelector('.wheel-content svg');
      if (svg) { observer.disconnect(); svg.append(document.createElement('a')); }
    });
    observer.observe(document.body, { childList: true, subtree: true });
  });
  await link.click();
  await page.locator('.chart-error').first().waitFor({ state: 'visible' });
  assert.equal(await page.locator('.wheel-stage').first().getAttribute('aria-busy'), 'false');
  assert.ok(await page.locator('.wheel .png').first().isDisabled());
  await page.getByRole('button', { name: 'Retry chart', exact: true }).first().click();
  await ready();
  console.log('PASS injected DOM failure: finite error state -> retry remount -> ready');

  await page.getByRole('link', { name: 'Research', exact: true }).click();
  await page.locator(`.recent-list a[href="/search/${pending}"]`).first().click();
  await page.waitForFunction(() => document.querySelector('main')?.textContent.includes('Chart test polling'));
  assert.equal(await page.locator('.wheel-svg').count(), 0);
  sql(`insert into suggestions(search_id,text,category,modifier,search_volume)
       values ('${pending}','how to brew coffee','questions','how',100) on conflict do nothing;
       update searches set status='done', suggestion_count=1 where id='${pending}';`);
  await ready();
  assert.equal(await page.evaluate(() => window.chartNavigationMarker), 'retained');
  console.log('PASS pending -> done polling: chart appears without reload');

  await page.setViewportSize({ width: 390, height: 844 });
  await ready();
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1));
  await page.emulateMedia({ reducedMotion: 'reduce' });
  assert.equal(await page.locator('.chart-loading .spinner').first().evaluate(e => getComputedStyle(e).animationName), 'none');
  const output = process.env.JCODE_SCRATCH_DIR;
  if (output) { await mkdir(output, { recursive: true }); await page.locator('.wheel').first().screenshot({ path: `${output}/chart-loading-fixed-mobile.png` }); }
  await page.goto(`${base}/search/${empty}`, { waitUntil: 'networkidle' });
  assert.equal(await page.locator('.wheel-stage').count(), 0);
  assert.equal(await page.locator('.chart-loading:visible').count(), 0);
  await Promise.all(httpChecks);
  assert.deepEqual(errors, []);
  console.log('PASS mobile, reduced motion, empty results, no hydration exceptions');

} finally {
  await browser.close();
}

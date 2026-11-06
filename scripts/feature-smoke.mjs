// Acceptance smoke test for a deliberately seeded, isolated test installation.
// Never clicks a provider-backed action. Requires scripts/npm install beforehand.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';

if (process.env.ATP_SMOKE_ALLOW_FIXTURES !== '1') {
  throw new Error('Set ATP_SMOKE_ALLOW_FIXTURES=1 only for the isolated fixture database.');
}
const base = process.env.ATP_FEATURE_BASE_URL || 'http://127.0.0.1:3017';
const searchId = '11111111-1111-4111-8111-111111111111';
const output = process.env.ATP_SMOKE_OUTPUT || process.env.JCODE_SCRATCH_DIR;
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
const errors = [];
page.on('pageerror', error => errors.push(error.message));
const waitUntil = async fn => {
  for (let i = 0; i < 100; i++) {
    if (await fn()) return;
    await page.waitForTimeout(100);
  }
  throw new Error('Timed out waiting for hydrated UI state.');
};
try {
  await page.goto(`${base}/search/${searchId}`, { waitUntil: 'networkidle' });
  const picker = page.locator('.prompt-discovery');
  await picker.waitFor();
  assert.match(await page.locator('main').innerText(), /ekspres do kawy/);
  const boxes = picker.locator('input[type=checkbox]');
  assert.ok(await boxes.count() >= 5, 'Questions, comparison, alternative and recommendation candidates exist');
  const run = picker.getByRole('button', { name: /^Ask .* approved prompts$/ });
  const approve = picker.getByRole('button', { name: 'Approve this prompt set', exact: true });
  if (await run.isEnabled()) {
    await picker.locator('input[type=checkbox]:checked').first().uncheck();
    await waitUntil(() => run.isDisabled());
    if (await picker.locator('input[type=checkbox]:checked').count() === 0) {
      await boxes.last().check();
    }
  }
  assert.ok(await run.isDisabled(), 'Unsaved selection cannot run');
  await approve.click();
  await waitUntil(() => run.isEnabled());
  assert.match(await picker.innerText(), /Approved\./);
  const approvedCount = await picker.locator('input[type=checkbox]:checked').count();
  assert.ok(approvedCount > 0 && approvedCount <= 20);
  await page.reload({ waitUntil: 'networkidle' });
  await picker.waitFor();
  await waitUntil(() => run.isEnabled());
  assert.equal(await picker.locator('input[type=checkbox]:checked').count(), approvedCount, 'Approval persists after reload');
  const checked = picker.locator('input[type=checkbox]:checked').first();
  await checked.uncheck();
  await waitUntil(() => run.isDisabled());
  await picker.getByRole('button', { name: 'Restore approved set', exact: true }).click();
  await waitUntil(() => run.isEnabled());
  const filter = picker.getByRole('searchbox', { name: 'Filter prompt candidates' });
  await filter.fill('Comparison');
  await waitUntil(async () => await picker.locator('fieldset > ul > li:visible').count() === 1);
  assert.ok(await run.isEnabled(), 'Filtering does not invalidate approval');
  assert.equal(await picker.locator('input[type=checkbox]:checked').count(), approvedCount);
  await filter.fill('');
  await picker.locator('summary').first().click();
  assert.match(await picker.innerText(), /Original:/);
  if (output) {
    await mkdir(output, { recursive: true });
    await picker.screenshot({ path: `${output}/atp-prompt-discovery.png` });
  }
  console.log('PASS discovery hydration, diverse candidates, explicit approval, reload, dirty-state guard, restore and provenance');

  await page.goto(`${base}/radar`, { waitUntil: 'networkidle' });
  const domain = page.locator('.radar-form input[name=domain]');
  const submit = page.getByRole('button', { name: 'Search local corpus', exact: true });
  await domain.fill('https://www.example.com/path');
  await submit.click();
  await page.locator('.radar-results').waitFor();
  await waitUntil(async () => /4 matching records/.test(await page.locator('.radar-results').innerText()));
  assert.equal(await page.locator('.radar-record').count(), 4);
  assert.match(await page.locator('.radar-results').innerText(), /6 records available/);
  assert.match(await page.locator('.radar-results').innerText(), /Mentioned in text only/i);
  await page.locator('input[name=subdomains]').check();
  await submit.click();
  await waitUntil(async () => /5 matching records/.test(await page.locator('.radar-results').innerText()));
  assert.equal(await page.locator('.radar-record').count(), 5, 'Subdomains included, lookalikes excluded');
  await page.locator('select[name=engine]').selectOption('google_aio');
  await submit.click();
  await waitUntil(async () => /1 matching records/.test(await page.locator('.radar-results').innerText()));
  assert.equal(await page.locator('.radar-record').count(), 1);
  await page.locator('input[name=country]').fill('zz');
  await submit.click();
  await waitUntil(async () => /No local answer records/.test(await page.locator('main').innerText()));
  await page.locator('input[name=country]').fill('');
  await page.locator('select[name=engine]').selectOption('');
  await domain.fill('absent.example');
  await submit.click();
  await waitUntil(async () => /No matching domain/.test(await page.locator('main').innerText()));
  await domain.fill('javascript:example.com');
  await submit.click();
  await page.locator('.error').waitFor();
  await domain.fill('example.com');
  await submit.click();
  await waitUntil(async () => (await page.locator('.radar-record').count()) === 5);
  if (output) await page.screenshot({ path: `${output}/atp-radar-desktop.png`, fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1), 'Radar has no mobile horizontal overflow');
  if (output) await page.screenshot({ path: `${output}/atp-radar-mobile.png`, fullPage: true });
  assert.deepEqual(errors, [], 'No client/hydration exceptions');
  console.log('PASS Radar domain normalization, source/text distinction, counts, subdomains, engine/market filters, empty/error states and mobile layout');
} finally {
  await browser.close();
}

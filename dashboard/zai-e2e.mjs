import {fileURLToPath} from 'node:url';
import { chromium } from 'playwright';
import fs from 'node:fs/promises';
import assert from 'node:assert/strict';

process.chdir(fileURLToPath(new URL('..', import.meta.url)));
const ports = JSON.parse(await fs.readFile(process.env.HYCLI_FIXTURE_OUTPUT || '.cache/e2e-ports.json', 'utf8'));
const en = JSON.parse(await fs.readFile('dashboard/src/locales/en.json', 'utf8'));
const browser = process.env.TEST_CDP ? await chromium.connectOverCDP(process.env.TEST_CDP) : await chromium.launch({ headless: true, executablePath: process.env.BROWSER_BIN, args: ['--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage'] });
const context = await browser.newContext({ viewport: { width: 1536, height: 1024 } });
let page;
const errors = [];
const button = (parent, key) => parent.getByRole('button', { name: en[key], exact: true });
const dialog = () => page.getByRole('dialog');
const keyInput = () => dialog().getByLabel(en['connections.key'], { exact: false });
const modelInput = () => dialog().getByLabel(en['connections.model'], { exact: false });
const choice = () => dialog().getByLabel(en['connections.provider'], { exact: true });
const state = () => page.evaluate(async () => (await fetch('/api/state')).json());
const save = async () => { await button(dialog(), 'common.save').click(); await dialog().waitFor({ state: 'hidden' }); };
const edit = id => button(page.locator(`.provider-card--${id}`), 'common.edit').click();
try {
  page = await context.newPage();
  page.setDefaultTimeout(60000);
  page.on('pageerror', error => errors.push(error.message));
  console.log('Checking separate Z.ai connections against the Rust fixture.');
  await page.goto(ports.dashboard, { waitUntil: 'domcontentloaded' });
  await button(page, 'nav.connections').click();
  assert.equal(await page.locator('.provider-card').count(), 5);
  await button(page.locator('.provider-card--zai'), 'connections.connect').click();
  assert.equal(await choice().inputValue(), 'zai');
  assert.deepEqual(await choice().locator('option').allTextContents(), ['Z.ai', 'Z.ai Coding Plan']);
  await keyInput().fill('synthetic-standard-key');
  await modelInput().fill('glm-5.3');
  await save();

  await button(page.locator('.provider-card--zai-coding-plan'), 'connections.connect').click();
  await keyInput().fill('unsaved-plan-key');
  await modelInput().fill('glm-5.3-flash');
  await choice().selectOption('zai');
  assert.equal(await keyInput().inputValue(), '');
  assert.equal(await keyInput().getAttribute('required'), null);
  assert.equal(await modelInput().inputValue(), 'glm-5.3');
  await choice().selectOption('zai-coding-plan');
  assert.equal(await keyInput().inputValue(), '');
  assert.notEqual(await keyInput().getAttribute('required'), null);
  assert.equal(await modelInput().inputValue(), 'glm-5.3');
  await keyInput().fill('synthetic-plan-key');
  await modelInput().fill('glm-5.3-flash');
  await dialog().getByLabel(en['connections.default'], { exact: true }).check();
  await save();

  let data = await state();
  assert.equal(data.providers.find(p => p.id === 'zai').model, 'glm-5.3');
  assert.equal(data.providers.find(p => p.id === 'zai-coding-plan').model, 'glm-5.3-flash');
  assert.equal(data.settings.default_provider, 'zai-coding-plan');
  for (const secret of ['synthetic-standard-key', 'synthetic-plan-key', 'unsaved-plan-key']) assert.ok(!JSON.stringify(data).includes(secret));
  // A blank key keeps that connection's saved key. The fixture rejects the other plan's key.
  await edit('zai');
  assert.equal(await keyInput().inputValue(), '');
  await save();
  await edit('zai-coding-plan');
  assert.equal(await keyInput().inputValue(), '');
  await save();

  // Disconnecting one plan must not disconnect the other.
  await edit('zai-coding-plan');
  await button(dialog(), 'connections.disconnect').click();
  await dialog().waitFor({ state: 'hidden' });
  data = await state();
  assert.equal(data.providers.find(p => p.id === 'zai').connected, true);
  assert.equal(data.providers.find(p => p.id === 'zai-coding-plan').has_key, false);
  // Saving after switching the selector must target its selection, not the card that opened it.
  await edit('zai');
  await choice().selectOption('zai-coding-plan');
  await keyInput().fill('synthetic-plan-key');
  await modelInput().fill('glm-5.3-flash');
  await save();
  data = await state();
  assert.equal(data.providers.find(p => p.id === 'zai-coding-plan').connected, true);
  assert.equal(data.providers.find(p => p.id === 'zai').model, 'glm-5.3');
  await page.screenshot({ path: 'docs/images/ai-connections.png', fullPage: true, animations: 'disabled' });
  await edit('zai-coding-plan');
  await page.screenshot({ path: 'docs/design/zai-plan-selection.png', animations: 'disabled' });
  await page.setViewportSize({ width: 390, height: 844 });
  assert.equal(await choice().isVisible(), true);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), true);
  await button(dialog(), 'common.cancel').click();
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), true);
  await page.screenshot({ path: 'docs/design/zai-connections-mobile.png', fullPage: true, animations: 'disabled' });
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ zai: 'passed', codingPlan: 'passed', selector: 'separate keys and models; no unsaved-key carryover', persistence: 'save, reload and disconnect isolated', mobile: 'no overflow', consoleErrors: 0 }, null, 2));
} finally {
  await context.close(); if (!process.env.TEST_CDP) await browser.close();
}

if (process.env.TEST_CDP) process.exit(process.exitCode || 0);

// Real production UI with a synthetic API and a deliberately delayed settings
// request. No real accounts, external requests, or disabled browser sandbox.
import { chromium } from 'playwright';
import { createServer } from 'node:http';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { selectPersistedLocale } from './e2e-settings.mjs';
const root = fileURLToPath(new URL('..', import.meta.url));
const web = process.env.LOCALE_WEB || path.join(root, 'web');
const baseline = process.env.LOCALE_BASELINE === '1';
const reportDir = path.join(root, 'test-results', 'locale-readiness');
await fs.mkdir(reportDir, {recursive:true});
const en = JSON.parse(await fs.readFile(path.join(root, 'dashboard/src/locales/en.json'), 'utf8'));
const state = { settings:{locale:'en',theme:'light'},providers:[],sites:[],accounts:[],jobs:[],approvals:[],engine:{available:false},version:'test' };
let writes = 0, rejectSave = false;
const server = createServer(async (req, res) => {
  if (req.url === '/api/events') { res.writeHead(200, {'Content-Type':'text/event-stream'}); res.end(': ready\n\n'); return; }
  if (req.url.startsWith('/api/')) {
    let body = ''; for await (const chunk of req) body += chunk;
    let result = {};
    if (req.url.startsWith('/api/session')) result = {csrf:'synthetic'};
    else if (req.url === '/api/state') result = state;
    else if (req.url === '/api/settings' && req.method === 'PATCH') {
      if (rejectSave) { res.writeHead(500, {'Content-Type':'application/json'}); res.end(JSON.stringify({error:{code:'internal'}})); return; }
      writes++; Object.assign(state.settings, JSON.parse(body)); result = state.settings;
    }
    res.writeHead(200, {'Content-Type':'application/json'}); res.end(JSON.stringify(result)); return;
  }
  const name = path.join(web,req.url === '/' ? 'index.html' : req.url.split('?')[0]);
  try { const bytes = await fs.readFile(name); res.writeHead(200, {'Content-Type':name.endsWith('.js')?'text/javascript':name.endsWith('.css')?'text/css':'text/html'}); res.end(bytes); }
  catch { res.writeHead(404); res.end(); }
});
await new Promise(resolve => server.listen(0,'127.0.0.1',resolve));
const origin = `http://127.0.0.1:${server.address().port}`;
let browser, context, releaseInitial;
try {
  browser = process.env.TEST_CDP ? await chromium.connectOverCDP(process.env.TEST_CDP) : await chromium.launch({headless:true, chromiumSandbox:true, executablePath:process.env.BROWSER_BIN});
  const open = async (holdInitial = false) => {
    const context = await browser.newContext();
    await context.route('**/*', route => new URL(route.request().url()).origin === origin ? route.continue() : route.abort());
    const page = await context.newPage(); page.setDefaultTimeout(10000);
    let releaseInitial;
    const held = new Promise(resolve => { releaseInitial = resolve; });
    const requested = holdInitial ? page.waitForRequest(request => new URL(request.url()).pathname === '/api/state') : null;
    if (holdInitial) {
      let first = true;
      await page.route('**/api/state', async route => {
        if (first) { first = false; await held; }
        await route.continue().catch(() => {});
      });
    }
    await page.goto(origin); await page.locator('.topbar select').waitFor();
    if (holdInitial) await requested;
    return {context,page,select:page.locator('.topbar select'),releaseInitial};
  };
  let page, select;
  ({context,page,select,releaseInitial} = await open(true));
  if (baseline) {
    assert.equal(await select.isDisabled(), false, 'Pinned pre-fix selector is interactive before settings arrive');
    await select.selectOption('ar');
    await page.waitForFunction(() => document.documentElement.lang === 'ar');
    assert.equal(writes, 0, 'Pre-fix early choice was never sent to settings');
    releaseInitial();
    await page.getByRole('heading',{name:en['sites.title'],exact:true}).waitFor();
    assert.equal(await select.inputValue(), 'en', 'Initial state overwrites the pre-fix early choice');
    assert.equal(state.settings.locale, 'en');
    await page.screenshot({path:path.join(reportDir,'baseline-lost-choice.png')});
    await fs.writeFile(path.join(reportDir,'baseline.json'),JSON.stringify({result:'expected-failure-reproduced',earlyChoice:'ar',persistedLocale:state.settings.locale,writes},null,2));
    console.log('EXPECTED RED: pinned pre-fix UI accepts an early locale choice, sends no settings request, then overwrites it.');
  } else {
    assert.equal(await select.isDisabled(), true, 'Locale selection must wait for initial persisted settings');
    assert.equal(writes, 0, 'No locale mutation while initial settings are unavailable');
    await page.screenshot({path:path.join(reportDir,'waiting-for-settings.png')});
    releaseInitial();
    await selectPersistedLocale(page,select,'ar');
    assert.equal(state.settings.locale,'ar');
    let intercepted;
    const requested = new Promise(resolve => { intercepted = resolve; });
    await page.route('**/api/settings', async route => {
      intercepted();
      await new Promise(resolve => setTimeout(resolve,300));
      await route.continue().catch(() => {}); // Closing a context aborts its in-flight request.
    });
    const restoring = selectPersistedLocale(page,select,'en');
    await requested;
    assert.equal(state.settings.locale,'ar','The controlled request must still be pending');
    await restoring;
    await context.close();
    assert.equal(state.settings.locale,'en','Closing a suite must not abandon the locale restore');
    ({context,page,select} = await open());
    await page.getByRole('heading',{name:en['sites.title'],exact:true}).waitFor();
    const before = writes;
    await selectPersistedLocale(page,select,'en');
    assert.equal(writes,before,'An already-persisted locale needs no mutation');
    rejectSave = true;
    await assert.rejects(selectPersistedLocale(page,select,'ar'), /Locale update must succeed/);
    assert.equal(state.settings.locale,'en','Failed settings requests must not be treated as persisted');
    await fs.writeFile(path.join(reportDir,'head.json'),JSON.stringify({result:'passed',delayedInitialState:true,delayedSave:true,noOp:true,failedSave:true},null,2));
    await context.close();
    console.log('PASS: delayed initial settings cannot lose a locale choice; delayed restore survives context teardown; no-op and failed-save checks pass.');
  }
} finally { releaseInitial?.(); await context?.close(); if (!process.env.TEST_CDP) await browser?.close(); server.closeAllConnections(); await new Promise(resolve => server.close(resolve)); }
if (process.env.TEST_CDP) process.exit(0);

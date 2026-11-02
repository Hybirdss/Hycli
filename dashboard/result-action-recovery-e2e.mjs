import { chromium, expect } from '@playwright/test';
import { createServer } from 'node:http';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';

const root = fileURLToPath(new URL('..', import.meta.url));
const web = process.env.RESULT_ACTION_WEB || path.join(root, 'web');
const baseline = process.env.RESULT_ACTION_BASELINE === '1';
const reportDir = path.join(root, 'test-results/result-action-recovery');
await fs.mkdir(reportDir, {recursive:true});
const en = JSON.parse(await fs.readFile(path.join(root, 'dashboard/src/locales/en.json'), 'utf8'));
const timestamp = '2026-10-20T12:00:00Z';
const artifact = {kind:'file',version:1,artifact_id:'0123456789abcdef0123456789abcdef',status:'complete',local_path:'/fixture/artifacts/payload',location_scope:'hycli_host',filename:'fixture.bin',media_type:'application/octet-stream',byte_count:12,sha256:'a'.repeat(64),created_at:timestamp,provenance:{site:'site',action:'download',source_origin:'https://example.test'},content_trust:'untrusted'};
const json = {message:'Original JSON result retained',count:3};
const jobs = ['file','json'].map(id => ({id,kind:'action',status:'completed',site_id:'site',site_title:'Example',action_title:id === 'file' ? 'Binary download' : 'Ordinary JSON',started_at:timestamp,updated_at:timestamp,finished_at:timestamp,found:[],notes:[],result:id === 'file' ? artifact : json,...(id === 'file' ? {artifact} : {})}));
const state = {settings:{locale:'en',theme:'light'},providers:[],sites:[{id:'site',title:'Example',url:'https://example.test',host:'example.test',status:'ready',actions:[],updated_at:timestamp}],accounts:[],jobs,approvals:[],engine:{available:true,compatible:true,version:'1'},version:'test'};
const requests = [], checks = [];
let exportFails = false, detailFails = false, holdExport = false, releaseExport;
const server = createServer(async (req,res) => {
  if (req.url === '/api/events') { res.writeHead(200,{'Content-Type':'text/event-stream'}); res.end(': ready\n\n'); return; }
  if (req.url.startsWith('/api/')) {
    requests.push({method:req.method,path:req.url});
    const send = (status,value) => { res.writeHead(status,{'Content-Type':'application/json'}); res.end(JSON.stringify(value)); };
    if (req.url.startsWith('/api/session')) { send(200,{csrf:'synthetic'}); return; }
    if (req.url === '/api/state') { send(200,{...state,jobs:jobs.map(({result,...snapshot}) => snapshot)}); return; }
    const target = jobs.find(job => job.id === req.url.split('/')[3]);
    if (req.method === 'GET' && target && req.url.endsWith('/result')) {
      const failed = exportFails;
      if (holdExport) await new Promise(resolve => { releaseExport = resolve; });
      send(failed ? 503 : 200,failed ? {error:{code:'connection_lost'}} : target.result); return;
    }
    if (req.method === 'GET' && target && req.url === `/api/jobs/${target.id}`) {
      send(detailFails ? 409 : 200,detailFails ? {error:{code:'data_invalid'}} : target); return;
    }
    send(404,{error:{code:'not_found'}}); return;
  }
  const name = path.join(web,req.url === '/' ? 'index.html' : req.url.split('?')[0]);
  try { const bytes = await fs.readFile(name); res.writeHead(200,{'Content-Type':name.endsWith('.js')?'text/javascript':name.endsWith('.css')?'text/css':'text/html'}); res.end(bytes); }
  catch { res.writeHead(404); res.end(); }
});
await new Promise(resolve => server.listen(0,'127.0.0.1',resolve));
const origin = `http://127.0.0.1:${server.address().port}`;
let browser;
try {
  browser = await chromium.launch({headless:true,chromiumSandbox:true,executablePath:process.env.BROWSER_BIN});
  const context = await browser.newContext({acceptDownloads:true});
  await context.route('**/*',route => new URL(route.request().url()).origin === origin ? route.continue() : route.abort());
  await context.addInitScript(() => {
    window.fixtureClipboard = {mode:'fail',writes:[]};
    window.installFixtureClipboard = () => Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async value => {
      const mode = window.fixtureClipboard.mode;
      window.fixtureClipboard.writes.push({mode,value});
      if (window.fixtureClipboard.hold) await new Promise(resolve => { window.fixtureClipboard.release = resolve; });
      if (mode === 'fail') throw new Error('Synthetic clipboard rejection');
    }}});
    window.installFixtureClipboard();
  });
  const page = await context.newPage(); page.setDefaultTimeout(10000);
  const errors = []; page.on('pageerror',error => errors.push(error.message));
  const button = key => page.getByRole('dialog').getByRole('button',{name:en[key],exact:true});
  const alert = code => page.getByRole('dialog').getByRole('alert').getByText(en['errors.'+code],{exact:true});
  const open = async id => {
    await page.goto(origin+'/#activity');
    await page.reload({waitUntil:'domcontentloaded'});
    const title = jobs.find(job => job.id === id).action_title;
    await page.locator('.activity-item').filter({hasText:title}).getByRole('button',{name:en['activity.viewResult'],exact:true}).click();
    await button(id === 'file' ? 'files.copyManifest' : 'actions.copy').waitFor();
  };
  const exportResult = async (id) => {
    const downloaded = page.waitForEvent('download');
    await button(id === 'file' ? 'files.exportManifest' : 'actions.download').click();
    const download = await downloaded;
    assert.equal(await download.failure(),null);
    assert.equal(download.suggestedFilename(),`hycli-${id}.json`);
    assert.deepEqual(JSON.parse(await fs.readFile(await download.path(),'utf8')),id === 'file' ? artifact : json);
  };
  for (const id of ['file','json']) {
    await open(id);
    await button(id === 'file' ? 'files.copyManifest' : 'actions.copy').click();
    await expect(alert('internal')).toHaveCount(1);
    await page.evaluate(() => { window.fixtureClipboard.mode = 'success'; });
    await button(id === 'file' ? 'files.copyManifest' : 'actions.copy').click();
    await expect.poll(() => page.evaluate(() => window.fixtureClipboard.writes.length)).toBe(2);
    const writes = await page.evaluate(() => window.fixtureClipboard.writes);
    assert.deepEqual(writes.map(write => write.mode),['fail','success']);
    for (const write of writes) assert.deepEqual(JSON.parse(write.value),id === 'file' ? artifact : json);
    await expect(alert('internal')).toHaveCount(baseline ? 1 : 0);
    await page.screenshot({path:path.join(reportDir,`${baseline ? 'baseline' : 'head'}-${id}-copy.png`)});
    checks.push(`${id} copy rejection followed by successful exact payload`);

    exportFails = true;
    await open(id);
    await button(id === 'file' ? 'files.exportManifest' : 'actions.download').click();
    await expect(alert('connection_lost')).toHaveCount(1);
    exportFails = false;
    await exportResult(id);
    await expect(alert('connection_lost')).toHaveCount(baseline ? 1 : 0);
    await page.screenshot({path:path.join(reportDir,`${baseline ? 'baseline' : 'head'}-${id}-export.png`)});
    checks.push(`${id} export rejection followed by successful exact downloaded JSON`);
  }
  if (!baseline) {
    for (const id of ['file','json']) {
      await open(id);
      await page.evaluate(() => Object.defineProperty(navigator,'clipboard',{configurable:true,value:undefined}));
      await button(id === 'file' ? 'files.copyManifest' : 'actions.copy').click();
      await expect(alert('internal')).toHaveCount(1);
      await page.evaluate(() => { window.installFixtureClipboard(); window.fixtureClipboard.mode = 'success'; });
      await button(id === 'file' ? 'files.copyManifest' : 'actions.copy').click();
      await expect.poll(() => page.evaluate(() => window.fixtureClipboard.writes.length)).toBe(1);
      assert.deepEqual(JSON.parse(await page.evaluate(() => window.fixtureClipboard.writes[0].value)),id === 'file' ? artifact : json);
      await expect(alert('internal')).toHaveCount(0);
      checks.push(`${id} unavailable clipboard recovers without remount`);

      await page.evaluate(() => { window.fixtureClipboard.mode = 'fail'; });
      await button(id === 'file' ? 'files.copyManifest' : 'actions.copy').click();
      await expect(alert('internal')).toHaveCount(1);
      exportFails = true;
      await button(id === 'file' ? 'files.exportManifest' : 'actions.download').click();
      await expect(alert('connection_lost')).toHaveCount(1);
      await expect(alert('internal')).toHaveCount(1);
      await page.evaluate(() => { window.fixtureClipboard.mode = 'success'; });
      await button(id === 'file' ? 'files.copyManifest' : 'actions.copy').click();
      await expect(alert('internal')).toHaveCount(0);
      await expect(alert('connection_lost')).toHaveCount(1);
      await page.evaluate(() => { window.fixtureClipboard.mode = 'fail'; });
      await button(id === 'file' ? 'files.copyManifest' : 'actions.copy').click();
      await expect(alert('internal')).toHaveCount(1);
      exportFails = false;
      await exportResult(id);
      await expect(alert('connection_lost')).toHaveCount(0);
      await expect(alert('internal')).toHaveCount(1);
      checks.push(`${id} recovery preserves the other action error in both directions`);
    }
    for (const id of ['file','json']) {
      await open(id);
      await page.evaluate(() => { window.fixtureClipboard.hold = true; });
      await button(id === 'file' ? 'files.copyManifest' : 'actions.copy').dblclick();
      await expect.poll(() => page.evaluate(() => window.fixtureClipboard.writes.length)).toBe(1);
      await page.evaluate(() => { window.fixtureClipboard.hold = false; window.fixtureClipboard.release(); });
      await expect(alert('internal')).toHaveCount(1);
      assert.equal(await page.evaluate(() => window.fixtureClipboard.writes.length),1,'Pending repeated copy has one external side effect');
      await page.evaluate(() => { window.fixtureClipboard.mode = 'success'; });
      await button(id === 'file' ? 'files.copyManifest' : 'actions.copy').click();
      await expect.poll(() => page.evaluate(() => window.fixtureClipboard.writes.length)).toBe(2);
      await expect(alert('internal')).toHaveCount(0);
      assert.deepEqual(JSON.parse(await page.evaluate(() => window.fixtureClipboard.writes[1].value)),id === 'file' ? artifact : json);

      exportFails = true; holdExport = true; releaseExport = undefined;
      const before = requests.filter(request => request.path === `/api/jobs/${id}/result`).length;
      await button(id === 'file' ? 'files.exportManifest' : 'actions.download').dblclick();
      await expect.poll(() => typeof releaseExport).toBe('function');
      holdExport = false; releaseExport();
      await expect(alert('connection_lost')).toHaveCount(1);
      assert.equal(requests.filter(request => request.path === `/api/jobs/${id}/result`).length,before+1,'Pending repeated export has one external request');
      exportFails = false;
      await exportResult(id);
      await expect(alert('connection_lost')).toHaveCount(0);
      assert.equal(requests.filter(request => request.path === `/api/jobs/${id}/result`).length,before+2,'Only the deliberate settled retry starts another export');
      checks.push(`${id} repeated pending copy and export are guarded, then settled failures can retry`);
    }
    detailFails = true;
    await open('file');
    await expect(alert('data_invalid')).toHaveCount(1);
    await page.evaluate(() => { window.fixtureClipboard.mode = 'success'; });
    await button('files.copyManifest').click();
    await expect.poll(() => page.evaluate(() => window.fixtureClipboard.writes.length)).toBe(1);
    await expect(alert('data_invalid')).toHaveCount(1);
    await exportResult('file');
    await expect(alert('data_invalid')).toHaveCount(1);
    checks.push('copy and export preserve unrelated result detail failure');
  }
  assert.deepEqual(errors,[],'No unhandled clipboard or download rejection');
  assert.equal(requests.filter(request => request.method !== 'GET').length,0,'Result recovery never reruns actions or invokes AI');
  await fs.writeFile(path.join(reportDir,baseline ? 'baseline.json' : 'head.json'),JSON.stringify({result:baseline ? 'expected-failure-reproduced' : 'passed',checks,requests},null,2));
  console.log(baseline ? 'EXPECTED RED: copy and export succeed but their earlier errors remain for file and JSON results.' : 'PASS: result action retries clear only their own errors, preserve unrelated failures, and never rerun actions.');
  await context.close();
} finally {
  await browser?.close();
  server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
}

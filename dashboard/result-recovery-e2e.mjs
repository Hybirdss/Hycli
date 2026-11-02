// Production dashboard against an isolated synthetic API, never a live service.
// Build first, then: BROWSER_BIN=/path/to/chromium node dashboard/result-recovery-e2e.mjs
// Synthetic file bytes only. No real credentials, external requests, or disabled browser sandbox.
import { chromium } from 'playwright';
import { createServer } from 'node:http';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
const root = fileURLToPath(new URL('..', import.meta.url));
const en = JSON.parse(await fs.readFile(path.join(root, 'dashboard/src/locales/en.json'), 'utf8'));
const now = () => new Date().toISOString();
const job = (id, status, extra = {}) => ({id:id.replaceAll(' ', '-'), kind:'action', status, site_id:'site', site_title:'Example', action_title:id, started_at:now(), updated_at:now(), finished_at:now(), found:[], notes:[], ...extra});
const state = { settings:{locale:'en', theme:'light', default_provider:'fixture'}, providers:[{id:'fixture',name:'Fixture AI',connected:true}], sites:[{id:'site',title:'Example',url:'https://example.test',host:'example.test',status:'ready',actions:[],updated_at:''}], accounts:[], jobs:[], approvals:[], engine:{available:true,compatible:true,version:'1'}, version:'test' };
let summaryCount = 0, hideNewest = false, refreshFails = false, holdSummary = false, releaseSummary;
const fileBytes = Buffer.from([137,80,78,71,13,10,26,10,0,255,128,65]);
let fileMissing = false, fileCorrupt = false, fileMismatch = false, expireArtifactSession = false, holdArtifactHead = false, releaseArtifactHead;
const requests = [], streams = new Set();
const assertHeadWaiting = async () => {
  const deadline = Date.now() + 10000;
  while (!releaseArtifactHead && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve,10));
  assert.equal(typeof releaseArtifactHead,'function','The native download waits for its HEAD preflight');
};
const changed = () => { for (const stream of streams) stream.write('data: changed\n\n'); };
const server = createServer(async (req, res) => {
  if (req.url === '/api/events') { res.writeHead(200, {'Content-Type':'text/event-stream'}); res.write(': ready\n\n'); streams.add(res); req.on('close', () => streams.delete(res)); return; }
  if (req.url.startsWith('/api/')) {
    let body = ''; for await (const chunk of req) body += chunk;
    const input = body ? JSON.parse(body) : {};
    requests.push({path:req.url,method:req.method,input});
    let result;
    if (req.url.startsWith('/api/session')) { res.setHeader('Set-Cookie', 'hycli_session=fixture; HttpOnly; SameSite=Strict; Path=/'); result = {csrf:'synthetic'}; }
    else if (req.url === '/api/state') {
      if (refreshFails) { res.writeHead(503, {'Content-Type':'application/json'}); res.end(JSON.stringify({error:{code:'connection_lost'}})); return; }
      result = {...state, jobs:state.jobs.filter(j => !hideNewest || j.id !== 'summary-'+summaryCount).map(({result, ...summary}) => summary)};
    } else if (req.url.includes('/artifacts/') && ['GET', 'HEAD'].includes(req.method)) {
      assert.match(req.headers.cookie || '', /hycli_session=fixture/, 'Native artifact requests use the existing session cookie');
      if (req.method === 'HEAD' && expireArtifactSession) { expireArtifactSession = false; res.writeHead(401); res.end(); return; }
      if (req.method === 'HEAD' && holdArtifactHead) await new Promise(resolve => { releaseArtifactHead = resolve; });
      const owner = state.jobs.find(j => j.id === req.url.split('/')[3]);
      if (fileMissing || !owner || owner.artifact?.artifact_id !== req.url.split('/')[5]) {
        res.writeHead(404, {'Content-Type':'application/json'}); res.end(JSON.stringify({error:{code:'not_found'}})); return;
      }
      if (fileMismatch) { res.writeHead(409, {'Content-Type':'application/json'}); res.end(JSON.stringify({error:{code:'data_invalid'}})); return; }
      res.writeHead(200, {'Content-Type':'application/octet-stream','Content-Disposition':'attachment; filename="fixture.bin"'});
      if (req.method === 'HEAD') { res.end(); return; }
      res.write(fileBytes);
      if (fileCorrupt) setTimeout(() => res.destroy(), 100);
      else res.end();
      return;
    } else if (req.url.endsWith('/result') && req.method === 'GET') result = state.jobs.find(j => j.id === req.url.split('/')[3])?.result;
    else if (req.url.startsWith('/api/jobs/') && req.method === 'GET') result = state.jobs.find(j => j.id === req.url.split('/')[3]);
    else if (req.url.endsWith('/summarize') || req.url.endsWith('/retry')) {
      const previous = state.jobs.find(j => j.id === req.url.split('/')[3]);
      assert.ok(previous, 'Exact target exists');
      if (req.url.endsWith('/retry')) assert.equal(previous.status,'failed');
      const child = job('summary-'+(++summaryCount), 'running', {kind:'summary', parent_id:previous.kind === 'summary' ? previous.parent_id : previous.id, provider:previous.kind === 'summary' ? previous.provider : input.provider, phase:'summarize'});
      state.jobs.unshift(child);
      if (holdSummary) await new Promise(resolve => { releaseSummary = resolve; });
      else await new Promise(r => setTimeout(r,150));
      result = {job:child};
    } else if (req.url === '/api/sites/site/actions/upload' && req.method === 'POST') {
      const approval = {id:'upload-approval-'+(state.approvals.length+1), status:'pending', site_id:'site', site_title:'Example', action_id:'upload', action_title:'Upload document', inputs:[{label:'Document', value:input.inputs.document}], expires_at:new Date(Date.now()+60000).toISOString()};
      state.approvals.push(approval);
      await new Promise(r => setTimeout(r,150));
      result = {approval};
    } else if (req.url.startsWith('/api/approvals/') && req.method === 'POST') {
      const approval = state.approvals.find(a => a.id === req.url.split('/')[3]);
      assert.equal(approval?.status,'pending','Only a pending synthetic approval can be resolved');
      if (req.url.endsWith('/reject')) { approval.status = 'rejected'; result = {}; }
      else if (req.url.endsWith('/approve')) {
        approval.status = 'approved';
        const completed = job('Upload document','completed',{result:{message:'Synthetic upload accepted', document:approval.inputs[0].value}});
        state.jobs.unshift(completed);
        await new Promise(r => setTimeout(r,150));
        result = {job:completed};
      } else { res.writeHead(404); res.end(); return; }
    } else { res.writeHead(404); res.end(); return; }
    res.writeHead(200, {'Content-Type':'application/json'}); res.end(JSON.stringify(result)); return;
  }
  const filename = path.join(root, 'web', req.url === '/' ? 'index.html' : decodeURIComponent(req.url.split('?')[0]));
  try { const bytes = await fs.readFile(filename); res.writeHead(200, {'Content-Type':filename.endsWith('.js')?'text/javascript':filename.endsWith('.css')?'text/css':filename.endsWith('.png')?'image/png':'text/html'}); res.end(bytes); } catch { res.writeHead(404); res.end(); }
});
await new Promise(r => server.listen(0, '127.0.0.1', r));
let browser;
try {
  browser = await chromium.launch({headless:true, chromiumSandbox:true, executablePath:process.env.BROWSER_BIN});
  const context = await browser.newContext();
  const origin = `http://127.0.0.1:${server.address().port}`;
  await context.route('**/*', route => new URL(route.request().url()).origin === origin ? route.continue() : route.abort());
  const page = await context.newPage(); page.setDefaultTimeout(10000);
  const errors = []; page.on('pageerror', e => errors.push(e.message));
  const button = (scope,key) => scope.getByRole('button',{name:en[key],exact:true});
  const dialog = () => page.getByRole('dialog');
  const visit = async () => { await page.goto(origin+'/#activity'); await page.reload({waitUntil:'domcontentloaded'}); };
  const open = async title => { await button(page.locator('.activity-item').filter({hasText:title}),'activity.viewResult').click(); };
  const close = async () => button(dialog(),'common.close').click();
  state.jobs = ['failed','cancelled','completed'].map(status => job('Empty '+status,status,status === 'failed' ? {error_code:'auth_required'} : {}));
  await visit();
  for (const status of ['failed','cancelled','completed']) {
    await open('Empty '+status);
    await dialog().locator('.result-heading').getByText(en['activity.'+status],{exact:true}).waitFor();
    if (status === 'completed') await dialog().getByText(en['actions.noResult'],{exact:true}).waitFor();
    else assert.equal(await dialog().getByText(en['actions.noResult'],{exact:true}).count(),0);
    if (status === 'failed') { await dialog().getByRole('alert').getByText(en['errors.auth_required'],{exact:true}).waitFor(); assert.equal(await button(dialog(),'accounts.autoFind').isVisible(),true); }
    await close();
  }
  console.log('PASS: empty-success text only on successful completion; failure remedy unchanged');
  state.jobs = [job('API denial','failed',{error_code:'api_denied'})];
  await visit();
  await open('API denial');
  await dialog().getByRole('alert').getByText(en['errors.api_denied'],{exact:true}).waitFor();
  assert.equal(await button(dialog(),'accounts.autoFind').count(),0,'API denial does not suggest reconnecting the account');
  assert.equal(await dialog().getByText(en['actions.noResult'],{exact:true}).count(),0,'A denied read is not an empty success');
  await close();
  console.log('PASS: API denial shows its own remedy without account reconnection or empty success');
  for (const code of ['oauth_recovery_required','oauth_verification_pending']) {
    const title = code === 'oauth_recovery_required' ? 'OAuth recovery' : 'OAuth verification';
    state.jobs = [job(title,'failed',{error_code:code})];
    const before = requests.length;
    await visit();
    await open(title);
    await dialog().locator('.result-heading').getByText(en['activity.failed'],{exact:true}).waitFor();
    await dialog().getByRole('alert').getByText(en['errors.'+code],{exact:true}).waitFor();
    assert.equal(await button(dialog(),'accounts.autoFind').count(),0,code+' does not suggest browser account discovery');
    assert.equal(await dialog().getByText(en['actions.noResult'],{exact:true}).count(),0,code+' is not an empty success');
    assert.equal(requests.slice(before).filter(r => r.method === 'POST' && !r.path.startsWith('/api/session')).length,0,code+' never starts automatic recovery work');
    await close();
  }
  console.log('PASS: OAuth failures show their localized remedies without browser discovery, automatic recovery or empty success');
  const original = job('Read original','completed',{result:{answer:'Original read retained'}});
  state.jobs = [original,job('Other parent','completed',{result:{answer:'Independent result'}}),job('unrelated-summary','failed',{kind:'summary',parent_id:'different-parent',error_code:'ai_billing_error'})];
  await visit();
  const openRead = async () => { await open('Read original'); await dialog().getByText('Original read retained',{exact:true}).waitFor(); };
  await openRead();
  assert.equal(await dialog().getByText(en['errors.ai_billing_error'],{exact:true}).count(),0);
  // An accepted response must keep the button guarded even before state catches up.
  hideNewest = true; holdSummary = true;
  await button(dialog(),'actions.summary').dblclick();
  const submissionDeadline = Date.now()+10000;
  while (!releaseSummary && Date.now()<submissionDeadline) await new Promise(r => setTimeout(r,10));
  assert.ok(releaseSummary,'Synthetic server accepted the pending POST');
  await close(); await openRead();
  assert.equal(await button(dialog(),'actions.summary').isEnabled(),false,'Pending POST stays guarded after remount');
  assert.equal(summaryCount,1);
  await close(); await open('Other parent');
  await dialog().getByText('Independent result',{exact:true}).waitFor();
  assert.equal(await button(dialog(),'actions.summary').isEnabled(),true,'A different parent is not blocked');
  await close(); await openRead();
  holdSummary = false; releaseSummary();
  await dialog().locator('.summary-operation').getByRole('progressbar').waitFor();
  assert.equal(summaryCount,1);
  assert.equal(await button(dialog(),'actions.summary').isEnabled(),false);
  hideNewest = false; changed();
  await close(); await openRead();
  await dialog().locator('.summary-operation').getByRole('progressbar').waitFor();
  const first = state.jobs.find(j => j.id === 'summary-1'); Object.assign(first,{status:'failed',error_code:'ai_unavailable'}); changed();
  await dialog().locator('.summary-operation').getByText(en['errors.ai_unavailable'],{exact:true}).waitFor();
  assert.equal(summaryCount,1,'No automatic retry');
  assert.equal(await dialog().getByText('Original read retained',{exact:true}).isVisible(),true);
  await close(); await openRead();
  await dialog().locator('.summary-operation').getByText(en['errors.no_summary'],{exact:true}).waitFor();
  state.settings.default_provider = 'different-default'; changed();
  await button(dialog().locator('.summary-operation'),'common.retry').dblclick();
  await dialog().locator('.summary-operation').getByRole('progressbar').waitFor();
  assert.equal(summaryCount,2);
  assert.equal(requests.filter(r => r.method === 'POST' && r.path === '/api/jobs/summary-1/retry').length,1);
  assert.equal(state.jobs.find(j => j.id === 'summary-2').parent_id,original.id);
  assert.equal(state.jobs.find(j => j.id === 'summary-2').provider,'fixture');
  Object.assign(state.jobs.find(j => j.id === 'summary-2'),{status:'completed'}); original.summary = 'A helpful explanation.'; changed();
  await dialog().getByText(original.summary,{exact:true}).waitFor();
  assert.equal(await dialog().locator('.summary-operation').count(),0);
  assert.equal(await dialog().getByText('Original read retained',{exact:true}).isVisible(),true);
  await close(); await openRead(); await dialog().getByText(original.summary,{exact:true}).waitFor();
  assert.equal(summaryCount,2);
  console.log('PASS: exact failure/retry/reopen, original result preserved, unrelated child ignored');
  // Cancellation uses summarize, since the existing retry endpoint only accepts failed jobs.
  await button(dialog(),'actions.summary').click();
  await dialog().locator('.summary-operation').getByRole('progressbar').waitFor();
  state.jobs.find(j => j.id === 'summary-3').status = 'cancelled'; changed();
  await dialog().locator('.summary-operation').getByText(en['activity.cancelled'],{exact:true}).waitFor();
  refreshFails = true;
  const failedRefresh = page.waitForResponse(response => new URL(response.url()).pathname === '/api/state' && response.status() === 503);
  await button(dialog().locator('.summary-operation'),'common.retry').dblclick();
  await failedRefresh;
  await page.locator('.main-content > .notice').getByText(en['activity.connectionLost'],{exact:true}).waitFor();
  assert.equal(await dialog().getByRole('alert').count(),0,'Accepted summary must not report a command failure when only snapshot synchronization fails');
  assert.equal(summaryCount,4,'Refresh failure never repeats accepted operation');
  assert.equal(requests.filter(r => r.path === '/api/jobs/summary-3/retry').length,0);
  await dialog().locator('.summary-operation').getByRole('progressbar').waitFor();
  assert.equal(await button(dialog(),'actions.summary').isEnabled(),false);
  await close(); await openRead();
  await dialog().locator('.summary-operation').getByRole('progressbar').waitFor();
  assert.equal(await button(dialog(),'actions.summary').isEnabled(),false,'Accepted operation stays guarded after failed-refresh remount');
  assert.equal(summaryCount,4);
  state.jobs.find(j => j.id === 'summary-4').status = 'completed';
  await close(); await openRead();
  assert.equal(await button(dialog(),'actions.summary').isEnabled(),false,'Even completed server work cannot be repeated through stale snapshot');
  assert.equal(summaryCount,4);
  refreshFails = false; changed();
  await dialog().locator('.summary-operation').waitFor({state:'hidden'});
  assert.equal(await page.getByText(en['activity.connectionLost'],{exact:true}).count(),0,'Successful reconciliation clears the separate stale notice');
  assert.equal(await dialog().getByRole('alert').count(),0,'Reconciliation leaves no false command error');
  // A later Activity retry is authoritative once the exact accepted child was observed.
  await button(dialog(),'actions.summary').click();
  await dialog().locator('.summary-operation').getByRole('progressbar').waitFor();
  Object.assign(state.jobs.find(j => j.id === 'summary-5'),{status:'failed',error_code:'ai_unavailable'}); changed();
  await dialog().locator('.summary-operation').getByText(en['errors.ai_unavailable'],{exact:true}).waitFor();
  await close();
  const activityRetry = page.waitForResponse(response => new URL(response.url()).pathname === '/api/jobs/summary-5/retry');
  await button(page.locator('.activity-item').filter({has:page.getByRole('heading',{name:'summary-5',exact:true})}),'common.retry').click();
  assert.equal((await activityRetry).status(),200);
  await page.getByRole('heading',{name:'summary-6',exact:true}).waitFor();
  await openRead();
  await dialog().locator('.summary-operation').getByRole('progressbar').waitFor();
  assert.equal(await dialog().getByText(en['errors.ai_unavailable'],{exact:true}).count(),0,'Activity retry supersedes the previously accepted failure');
  assert.equal(state.jobs.find(j => j.id === 'summary-6').parent_id,original.id);
  state.jobs.find(j => j.id === 'summary-6').status = 'completed'; original.summary = 'Explanation from Activity retry.'; changed();
  await dialog().getByText(original.summary,{exact:true}).waitFor();
  assert.equal(await dialog().locator('.summary-operation').count(),0,'Newer success leaves no stale failure or Retry');
  await close(); await openRead();
  await dialog().getByText(original.summary,{exact:true}).waitFor();
  assert.equal(await dialog().locator('.summary-operation').count(),0);
  assert.equal(summaryCount,6);
  console.log('PASS: Activity retry supersedes observed accepted child and clears old failure');
  const manifest = {kind:'file',version:1,artifact_id:'0123456789abcdef0123456789abcdef',status:'complete',local_path:'/fixture/hycli/artifacts/0123456789abcdef0123456789abcdef/payload',location_scope:'hycli_host',filename:'fixture.bin',media_type:'application/octet-stream',byte_count:fileBytes.length,sha256:createHash('sha256').update(fileBytes).digest('hex'),created_at:now(),provenance:{site:'site',action:'download',source_origin:'https://example.test'},content_trust:'untrusted'};
  state.jobs = [job('Ordinary JSON','completed',{result:manifest})];
  await visit(); await open('Ordinary JSON');
  await dialog().locator('.result-fields').first().waitFor();
  assert.equal(await dialog().getByRole('region',{name:en['files.ready']}).count(),0,'Server JSON cannot mint a local file card');
  assert.equal(await button(dialog(),'actions.summary').isEnabled(),true,'Ordinary JSON keeps its existing summary behavior');
  assert.equal(await button(dialog(),'files.download').count(),0);
  await close();
  const secondManifest = {...manifest,artifact_id:'abcdef0123456789abcdef0123456789ab',local_path:'/fixture/hycli/artifacts/abcdef0123456789abcdef0123456789ab/payload'};
  state.jobs = [job('Binary download','completed',{result:manifest,artifact:manifest}), job('Second download','completed',{result:secondManifest,artifact:secondManifest}), job('Not a file','completed',{result:manifest}), job('Running file','running',{artifact:manifest})];
  const fileStart = requests.length;
  await visit(); await button(page,'files.filter').click();
  const historyFiles = () => page.locator('.file-history .file-result');
  await historyFiles().nth(1).waitFor();
  assert.equal(await historyFiles().count(),2,'Only completed action artifacts enter the file list');
  await page.getByText(en['files.listScope'],{exact:true}).waitFor();
  for (const [index, metadata] of [manifest,secondManifest].entries()) {
    const row = historyFiles().nth(index);
    assert.equal(await row.getAttribute('data-artifact-id'),metadata.artifact_id);
    await row.getByRole('heading',{name:'fixture.bin',exact:true}).waitFor();
    for (const value of [metadata.artifact_id,metadata.local_path,en['files.notInspected'],en['files.notChecked']]) await row.getByText(value,{exact:true}).waitFor();
    await row.getByText('https://example.test · site / download',{exact:true}).waitFor();
  }
  await fs.mkdir(path.join(root,'test-results/file-download'),{recursive:true});
  await page.screenshot({path:path.join(root,'test-results/file-download/available-files.png'),fullPage:true});
  await page.reload({waitUntil:'domcontentloaded'}); await button(page,'files.filter').click();
  await historyFiles().nth(1).waitFor();
  assert.equal(await historyFiles().count(),2,'The available file list survives reopening from typed job history');
  const secondDownload = page.waitForEvent('download');
  await button(historyFiles().nth(1),'files.download').click();
  const selectedSecond = await secondDownload;
  assert.equal(selectedSecond.url(),origin+'/api/jobs/Second-download/artifacts/'+secondManifest.artifact_id,'Same-name selection preserves artifact and job identity');
  assert.deepEqual(await fs.readFile(await selectedSecond.path()),fileBytes);
  state.jobs = state.jobs.filter(j => j.id !== 'Second-download'); changed();
  await historyFiles().nth(1).waitFor({state:'hidden'});
  assert.equal(await historyFiles().count(),1,'Pruned records disappear without scanning or deleting payloads');
  await button(page,'common.all').click(); await open('Binary download');
  await dialog().getByRole('region',{name:en['files.ready']}).waitFor();
  await dialog().getByText(en['files.localOnly'],{exact:true}).waitFor();
  assert.equal(await button(dialog(),'actions.summary').count(),0,'File metadata is not offered as a content summary');
  for (let i = 0; i < 2; i++) {
    const before = requests.length;
    if (i === 0) { expireArtifactSession = true; holdArtifactHead = true; }
    const pending = page.waitForEvent('download');
    if (i === 0) {
      await button(dialog(),'files.download').dblclick();
      assert.equal(await button(dialog(),'files.download').isDisabled(),true);
      await assertHeadWaiting();
      holdArtifactHead = false; releaseArtifactHead(); releaseArtifactHead = undefined;
    } else await button(dialog(),'files.download').click();
    const received = await pending;
    assert.equal(received.url(),origin+'/api/jobs/Binary-download/artifacts/'+manifest.artifact_id,'The browser downloads directly from the authenticated route, not a blob URL');
    assert.equal(received.suggestedFilename(),'fixture.bin');
    assert.deepEqual(await fs.readFile(await received.path()),fileBytes,'Browser retrieves exact binary bytes');
    await dialog().getByText(en['files.started'],{exact:true}).waitFor();
    await dialog().getByText(en['files.available'],{exact:true}).waitFor();
    assert.equal(requests.slice(before).filter(r => r.path.includes('/artifacts/') && r.method === 'GET').length,1,'Repeated clicks while checking launch one native download');
    if (i === 0) {
      assert.equal(requests.slice(before).filter(r => r.path === '/api/session').length,1,'HEAD expiry renews the existing session once');
      await close(); await open('Binary download'); await dialog().getByRole('region',{name:en['files.ready']}).waitFor();
      await dialog().getByText(en['files.notChecked'],{exact:true}).waitFor();
    }
  }
  const exported = page.waitForEvent('download');
  await button(dialog(),'files.exportManifest').click();
  assert.deepEqual(JSON.parse(await fs.readFile(await (await exported).path(),'utf8')),manifest,'JSON export remains the file manifest');
  await fs.mkdir(path.join(root,'test-results/file-download'),{recursive:true});
  await page.screenshot({path:path.join(root,'test-results/file-download/completed-file.png'),fullPage:true});
  fileCorrupt = true;
  const failedDownload = page.waitForEvent('download');
  await button(dialog(),'files.download').click();
  assert.ok(await (await failedDownload).failure(),'A streaming body error is a browser download failure, not a completed save');
  await dialog().getByText(en['files.started'],{exact:true}).waitFor();
  await dialog().getByText(en['files.notInspected'],{exact:true}).waitFor();
  fileCorrupt = false; fileMissing = true;
  const getsBeforeMissing = requests.filter(r => r.method === 'GET' && r.path.includes('/artifacts/')).length;
  const missing = page.waitForResponse(response => response.request().method() === 'HEAD' && new URL(response.url()).pathname.includes('/artifacts/'));
  await button(dialog(),'files.download').click();
  assert.equal((await missing).status(),404);
  await dialog().getByText(en['files.unavailable'],{exact:true}).waitFor();
  await dialog().getByRole('alert').waitFor();
  assert.equal(await dialog().getByText(en['files.started'],{exact:true}).count(),0,'A failed preflight does not claim download started');
  assert.equal(requests.filter(r => r.method === 'GET' && r.path.includes('/artifacts/')).length,getsBeforeMissing,'Missing files do not start a native GET');
  fileMissing = false; fileMismatch = true;
  const mismatch = page.waitForResponse(response => response.request().method() === 'HEAD' && new URL(response.url()).pathname.includes('/artifacts/'));
  await button(dialog(),'files.download').click();
  assert.equal((await mismatch).status(),409);
  await dialog().getByText(en['files.unavailable'],{exact:true}).waitFor();
  await dialog().getByRole('alert').getByText(en['errors.data_invalid'],{exact:true}).waitFor();
  assert.equal(requests.filter(r => r.method === 'GET' && r.path.includes('/artifacts/')).length,getsBeforeMissing,'A manifest conflict cannot start a native GET');
  fileMismatch = false;
  const availability = page.waitForResponse(response => response.request().method() === 'HEAD' && new URL(response.url()).pathname.includes('/artifacts/'));
  await button(dialog(),'files.check').click(); await availability;
  await dialog().getByText(en['files.available'],{exact:true}).waitFor();
  assert.equal(await dialog().getByText(en['files.started'],{exact:true}).count(),0,'Availability check is not a download or content inspection');
  holdArtifactHead = true;
  await button(dialog(),'files.download').click();
  await assertHeadWaiting();
  await close(); holdArtifactHead = false; releaseArtifactHead(); releaseArtifactHead = undefined;
  await open('Binary download'); await dialog().getByText(en['files.notChecked'],{exact:true}).waitFor();
  assert.equal(requests.filter(r => r.method === 'GET' && r.path.includes('/artifacts/')).length,getsBeforeMissing,'Closing while checking aborts a delayed download');
  assert.equal(requests.slice(fileStart).filter(r => r.method === 'POST').length,0,'Listing, retrieval, failure and reopen never rerun an action or call AI');
  await close();
  await fs.writeFile(path.join(root,'test-results/file-download/report.json'),JSON.stringify({
    result:'passed',environment:'production dashboard with synthetic local HTTP fixtures',
    liveExternalAccounts:false,typedHistoryOnly:true,sameNameIdentity:true,reopenAndPruning:true,
    nativeBytesVerified:true,duplicateClickGuard:true,existingSessionRenewal:true,
    corruptStreamRejected:true,missingAndMismatchedFilesRejected:true,closeAbortsPendingDownload:true,
    separateManifestExport:true,actionReruns:0,contentInspectionClaimed:false
  },null,2));
  console.log('PASS: scoped typed file list, same-name identities, resume/pruning, native cookie download, duplicate guard, renewal, corruption/missing/close failures, separate manifest export and no action rerun');
  const uploadPath = '/synthetic host/Project Files/report final.pdf';
  state.sites[0].writes = true;
  state.sites[0].actions = [{id:'upload',title:'Upload document',description:'Send a document to the synthetic service.',effect:'write',verified:true,accepts_account:false,inputs:[{id:'document',label:'Document',type:'file',required:true,hint:'PDF document.'}]}];
  const actionPosts = () => requests.filter(r => r.method === 'POST' && r.path === '/api/sites/site/actions/upload');
  const approvalPosts = suffix => requests.filter(r => r.method === 'POST' && r.path.startsWith('/api/approvals/') && r.path.endsWith(suffix));
  const openUpload = async () => {
    await page.goto(origin+'/#websites');
    await page.reload({waitUntil:'domcontentloaded'});
    await page.getByRole('button',{name:'Example',exact:true}).click();
    await dialog().getByRole('button',{name:/Upload document/}).click();
  };
  await openUpload();
  await dialog().getByText('PDF document. '+en['actions.filePathHint'],{exact:true}).waitFor();
  const fileInput = () => dialog().getByRole('textbox');
  await button(dialog(),'actions.review').click();
  assert.equal(await fileInput().evaluate(element => element.validity.valueMissing),true,'Required file path is validated before submission');
  assert.equal(actionPosts().length,0,'Missing path sends no action POST');
  await fileInput().fill(uploadPath);
  assert.equal(await fileInput().inputValue(),uploadPath,'Spaces in explicit host path survive input');
  await button(dialog(),'common.cancel').click();
  assert.equal(actionPosts().length,0,'Cancelling input sends no action POST');
  await openUpload();
  await fileInput().fill(uploadPath);
  await button(dialog(),'actions.review').dblclick();
  await dialog().locator('.review-details').getByText(uploadPath,{exact:true}).waitFor();
  assert.equal(actionPosts().length,1,'Repeated review clicks create only one approval');
  assert.equal(actionPosts()[0].input.inputs.document,uploadPath,'Exact user path reaches action request');
  await button(dialog(),'common.cancel').click();
  await dialog().waitFor({state:'hidden'});
  assert.equal(approvalPosts('/reject').length,1);
  assert.equal(approvalPosts('/approve').length,0,'Cancelling approval performs no upload');
  await openUpload();
  await fileInput().fill(uploadPath);
  await button(dialog(),'actions.review').dblclick();
  await dialog().locator('.review-details').getByText(uploadPath,{exact:true}).waitFor();
  await close();
  await page.locator('.approval-banner').click();
  await dialog().locator('.review-details').getByText(uploadPath,{exact:true}).waitFor();
  const screenshotDir = path.join(root,'test-results/file-upload');
  await fs.mkdir(screenshotDir,{recursive:true});
  await dialog().screenshot({path:path.join(screenshotDir,'synthetic-approval.png')});
  await button(dialog(),'safety.allowOnce').dblclick();
  await dialog().getByText('Synthetic upload accepted',{exact:true}).waitFor();
  assert.equal(approvalPosts('/approve').length,1,'Repeated approval clicks execute once');
  assert.equal(actionPosts().length,2,'Only the two explicit review submissions create actions');
  await dialog().screenshot({path:path.join(screenshotDir,'synthetic-result.png')});
  await close();
  await visit();
  await open('Upload document');
  await dialog().getByText('Synthetic upload accepted',{exact:true}).waitFor();
  await dialog().getByText(uploadPath,{exact:true}).waitFor();
  await close(); await open('Upload document');
  await dialog().getByText('Synthetic upload accepted',{exact:true}).waitFor();
  assert.equal(actionPosts().length,2,'Result close/reopen never repeats the action');
  assert.equal(approvalPosts('/approve').length,1,'Result close/reopen never repeats approval');
  console.log('PASS: synthetic upload host-path input, required validation, cancel/reopen, exact approval and result, repeated-click guards; no real file transfer claimed');
  assert.deepEqual(errors,[]);
  console.log('PASS: cancelled summary explicit retry and accepted-write/failed-refresh guard');
  console.log('RESULT: synthetic production-dashboard regression passed; not live-service verification');
} finally { if (browser) await browser.close(); server.closeAllConnections(); await new Promise(r => server.close(r)); }

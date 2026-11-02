// Runs the built production app against synthetic data, with the browser sandbox enabled.
// ACCEPTED_COMMAND_BASELINE=1 verifies the exact pinned pre-fix failure, not a rewritten handler.
import { chromium } from 'playwright';
import { createServer } from 'node:http';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
const root = fileURLToPath(new URL('..', import.meta.url));
const web = process.env.ACCEPTED_COMMAND_WEB || path.join(root, 'web');
const baseline = process.env.ACCEPTED_COMMAND_BASELINE === '1';
const reportDir = path.join(root, 'test-results', 'accepted-command');
await fs.mkdir(reportDir, {recursive:true});
const en = JSON.parse(await fs.readFile(path.join(root, 'dashboard/src/locales/en.json'), 'utf8'));
const now = () => new Date().toISOString();
const job = (id, extra = {}) => ({id, kind:'action', status:'running', phase:'run', site_id:'site', site_title:'Example', action_title:id, started_at:now(), updated_at:now(), found:[], notes:[], ...extra});
const action = {id:'read', title:'Read fixture', description:'Synthetic read', output:'', effect:'read', verified:true, inputs:[]};
const site = {id:'site', title:'Example', url:'https://example.test', host:'example.test', summary:'Synthetic site', status:'ready', pinned:false, writes:true, actions:[action], updated_at:now()};
const initial = () => ({settings:{locale:'en',theme:'light',default_provider:'fixture'},providers:[{id:'fixture',name:'Fixture AI',connected:true}],sites:[],accounts:[],jobs:[],approvals:[],engine:{available:true,compatible:true,version:'1'},version:'initial'});
let state = initial(), sync = 'ok', command = 'accept', holdPost = false, releasePost, reads = 0, captureRead = false, capturedRead, holdStreamOpen = false;
const posts = [], streams = new Set(), heldReads = new Set(), results = [];
const changed = () => { for (const stream of streams) stream.write('data: changed\n\n'); };
const reply = (res, value, status=200) => {res.writeHead(status, {'Content-Type':'application/json'});res.end(JSON.stringify(value));};
const server = createServer(async (req,res) => {
  if (req.url === '/api/events') {res.writeHead(200,{'Content-Type':'text/event-stream'});if(!holdStreamOpen)res.write(': ready\n\n');streams.add(res);req.on('close',()=>streams.delete(res));return;}
  if (req.url.startsWith('/api/')) {
    let body='';for await(const chunk of req)body+=chunk;
    if (req.url.startsWith('/api/session')) return reply(res,{csrf:'synthetic'});
    if (req.url === '/api/state') {
      reads++;
      if(captureRead){captureRead=false;capturedRead={res,value:structuredClone(state)};return;}
      if(sync==='fail')return reply(res,{error:{code:'connection_lost'}},503);
      if(sync==='hang'){heldReads.add(res);res.on('close',()=>heldReads.delete(res));return;}
      return reply(res,state);
    }
    if(req.method==='GET' && req.url.startsWith('/api/jobs/')) return reply(res,state.jobs.find(j=>j.id===req.url.split('/')[3]));
    if(req.method==='POST') {
      posts.push({path:req.url,body:body?JSON.parse(body):null});
      if(holdPost)await new Promise(resolve=>{releasePost=resolve;});
      if(command==='reject')return reply(res,{error:{code:'ai_unavailable'}},503);
      let accepted = job(`accepted-${posts.length}`);
      if(req.url.endsWith('/summarize')) accepted=job(`summary-${posts.length}`,{kind:'summary',parent_id:req.url.split('/')[3],phase:'summarize'});
      if(!req.url.endsWith('/reject'))state.jobs.unshift(accepted);
      state.version=`accepted-${posts.length}`;
      return reply(res,req.url.endsWith('/reject')?{ok:true}:{job:accepted});
    }
    return reply(res,{error:{code:'not_found'}},404);
  }
  const filename=path.join(web, req.url==='/'?'index.html':decodeURIComponent(req.url.split('?')[0]));
  try{const bytes=await fs.readFile(filename);res.writeHead(200,{'Content-Type':filename.endsWith('.js')?'text/javascript':filename.endsWith('.css')?'text/css':filename.endsWith('.png')?'image/png':'text/html'});res.end(bytes);}catch{res.writeHead(404);res.end();}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const wait = async predicate => {const deadline=Date.now()+10000;while(!await predicate()){assert.ok(Date.now()<deadline,'Fixture condition timed out');await new Promise(resolve=>setTimeout(resolve,10));}};
let browser, page;
try {
  browser=await chromium.launch({headless:true,chromiumSandbox:true,executablePath:process.env.BROWSER_BIN});
  const context=await browser.newContext();const origin=`http://127.0.0.1:${server.address().port}`;
  let prepareAttempts = 0;
  await context.route('**/*',route => {
    const request = route.request();
    const target = new URL(request.url());
    if(target.origin !== origin)return route.abort();
    if(target.pathname === '/api/sites/prepare' && request.method() === 'POST') {
      prepareAttempts++;
      // Reject fetch deterministically, without HTTP socket retry behavior.
      if(command === 'ambiguous')return route.abort('failed');
    }
    return route.continue();
  });
  page=await context.newPage();page.setDefaultTimeout(10000);const errors=[];page.on('pageerror',e=>errors.push(e.message));
  const button=(scope,key)=>scope.getByRole('button',{name:en[key],exact:true});
  const url=()=>page.getByRole('textbox',{name:en['sites.urlLabel'],exact:true});
  const intent=()=>page.getByRole('textbox',{name:en['sites.intentLabel'],exact:true});
  const connect=()=>button(page,'sites.connect');const dialog=()=>page.getByRole('dialog');
  const frame=()=>page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
  const record=name=>{results.push(name);console.log(`PASS: ${name}`);};
  async function reset(next=initial(), hash='', deferStream=false) {
    capturedRead?.res.destroy();capturedRead=undefined;captureRead=false;for(const res of heldReads)res.destroy();heldReads.clear();sync='ok';command='accept';holdPost=false;releasePost=undefined;posts.length=0;prepareAttempts=0;state=next;holdStreamOpen=deferStream;
    await page.goto(origin+'/'+hash);
    // A hash-only goto preserves Workspace state; each fixture needs a new document.
    await page.reload({waitUntil:'domcontentloaded'});
    await page.locator('.local-status').getByText('vinitial',{exact:true}).waitFor();await wait(()=>streams.size>0);await frame();
  }
  async function prepare(mode, edit=false) {
    await reset();await url().fill('https://example.test');await intent().fill('Original draft');
    sync=mode;holdPost=true;const previousReads=reads;
    await connect().click();await wait(()=>releasePost);
    if(edit)await intent().fill('Newer edited draft');
    holdPost=false;releasePost();await wait(()=>reads>previousReads);
  }
  await prepare('fail');await wait(()=>connect().isEnabled());
  const originalAssertion=()=>assert.equal(urlValue,'','An accepted command clears its unchanged draft even when the following state read fails');
  const urlValue=await url().inputValue();
  if(baseline) {
    // The same success assertion used below must fail specifically for this defect.
    assert.throws(originalAssertion, {code:'ERR_ASSERTION'});
    assert.equal(urlValue,'https://example.test');assert.equal(await intent().inputValue(),'Original draft');
    await page.getByRole('alert').getByText(en['errors.connection_lost'],{exact:true}).waitFor();
    assert.equal(posts.length,1);assert.equal(state.jobs.length,1);
    await page.screenshot({path:path.join(reportDir,'baseline-false-failure.png'),fullPage:true});
    record('RED reproduced: accepted POST persists one job, subsequent GET failure falsely retains draft and reports command failure');
  } else {
    originalAssertion();assert.equal(await intent().inputValue(),'');assert.equal(await page.getByRole('alert').count(),0);assert.equal(posts.length,1);
    await page.getByText(en['activity.connectionLost'],{exact:true}).waitFor();
    await page.screenshot({path:path.join(reportDir,'fixed-accepted-stale.png'),fullPage:true});
    record('Accepted POST plus failed GET clears unchanged draft without a command error and shows the separate stale notice');
    sync='ok';state.version='reconciled';changed();await page.locator('.local-status').getByText('vreconciled',{exact:true}).waitFor();assert.equal(await page.getByText(en['activity.connectionLost'],{exact:true}).count(),0);assert.equal(posts.length,1);record('Later snapshot reconciles without replay');
    await reset(initial(),'',true);sync='fail';await url().fill('https://example.test');await connect().click();await wait(()=>posts.length===1);await wait(()=>connect().isEnabled());await page.getByText(en['activity.connectionLost'],{exact:true}).waitFor();
    const streamResponse=page.waitForResponse(r=>new URL(r.url()).pathname==='/api/events');for(const stream of streams)stream.write(': delayed open\n\n');await streamResponse;await frame();
    assert.equal(await page.getByText(en['activity.connectionLost'],{exact:true}).count(),1);record('EventSource opening cannot clear failed snapshot freshness');
    await prepare('hang');await wait(()=>connect().isEnabled());assert.equal(await url().inputValue(),'');assert.equal(posts.length,1);assert.ok(heldReads.size);record('Hanging GET does not hold accepted command or busy state hostage');
    await prepare('fail',true);await wait(()=>connect().isEnabled());assert.equal(await url().inputValue(),'https://example.test');assert.equal(await intent().inputValue(),'Newer edited draft');assert.equal(await page.getByRole('alert').count(),0);record('Accepted command preserves draft edits made while POST was pending');
    for(const outcome of ['reject','ambiguous']) {
      await reset();command=outcome;await url().fill('https://example.test');await intent().fill('Keep rejected draft');await connect().click();
      await page.getByRole('alert').getByText(en[outcome==='reject'?'errors.ai_unavailable':'errors.connection_lost'],{exact:true}).waitFor().catch(error => {
        throw new Error(`${outcome}: expected command error; observed ${prepareAttempts} browser POST attempts and ${posts.length} server POSTs`, {cause:error});
      });
      await wait(()=>connect().isEnabled());
      const assertAttempts = () => {
        assert.equal(prepareAttempts,1,`${outcome}: expected exactly one browser POST attempt, observed ${prepareAttempts}`);
        const expectedServerPosts = outcome === 'reject' ? 1 : 0;
        assert.equal(posts.length,expectedServerPosts,`${outcome}: expected ${expectedServerPosts} server POSTs, observed ${posts.length}`);
      };
      assertAttempts();assert.equal(await intent().inputValue(),'Keep rejected draft');
      const reconciled = page.waitForResponse(response => new URL(response.url()).pathname === '/api/state' && response.status() === 200);
      changed();await reconciled;await frame();assertAttempts();
      assert.equal(await page.getByRole('alert').getByText(en[outcome==='reject'?'errors.ai_unavailable':'errors.connection_lost'],{exact:true}).count(),1);
      assert.equal(await intent().inputValue(),'Keep rejected draft');
      record(`${outcome} command preserves error and draft without automatic replay`);
    }
    await reset();captureRead=true;changed();await wait(()=>capturedRead);
    await url().fill('https://example.test');await connect().click();await wait(()=>connect().isEnabled());await page.locator('.local-status').getByText('vaccepted-1',{exact:true}).waitFor();
    const oldResponse=page.waitForResponse(r=>new URL(r.url()).pathname==='/api/state');reply(capturedRead.res,capturedRead.value);await oldResponse;capturedRead=undefined;await frame();
    assert.equal(await page.locator('.local-status').getByText('vaccepted-1',{exact:true}).count(),1);assert.equal(posts.length,1);record('Pre-command GET completing after accepted command cannot overwrite newer published snapshot');
    await reset({...initial(),sites:[site]});await page.locator('.site-title').click();await dialog().locator('.action-row').click();sync='fail';await button(dialog(),'actions.run').click();
    await dialog().getByRole('progressbar').waitFor();assert.equal(posts.length,1);assert.equal(await dialog().getByRole('alert').count(),0);record('Action receipt opens its selected result while snapshot reads fail');
    sync='ok';Object.assign(state.jobs[0],{status:'completed',result:'Authoritative snapshot result',finished_at:now()});changed();await dialog().getByText('Authoritative snapshot result',{exact:true}).waitFor();
    state.jobs=[];state.version='removed';changed();await page.locator('.local-status').getByText('vremoved',{exact:true}).waitFor();await wait(async()=>await dialog().count()===0);record('Exact observed snapshot retires selected receipt permanently');
    const approval={id:'approval',site_id:'site',site_title:'Example',action_id:'write',action_title:'Synthetic change',inputs:[],expires_at:new Date(Date.now()+600000).toISOString(),status:'pending'};
    for(const decision of ['approve','reject']) {
      await reset({...initial(),sites:[site],approvals:[approval]});await page.locator('.approval-banner').click();sync='fail';await button(dialog(),decision==='approve'?'safety.allowOnce':'common.cancel').click();await wait(()=>posts.length===1);
      if(decision==='approve'){await dialog().getByRole('progressbar').waitFor();await button(dialog(),'common.close').click();}else await wait(async()=>await dialog().count()===0);
      assert.equal(await page.locator('.approval-banner').count(),0);sync='ok';state.version='stale-pending';changed();await page.locator('.local-status').getByText('vstale-pending',{exact:true}).waitFor();assert.equal(await page.locator('.approval-banner').count(),0);assert.equal(posts.length,1);record(`Successful ${decision} suppresses handled approval even in a stale pending snapshot`);
    }
    const parent=job('parent',{status:'completed',result:'Original result',finished_at:now()});
    await reset({...initial(),jobs:[parent]},'#activity');await button(page.locator('.activity-item'),'activity.viewResult').click();await dialog().getByText('Original result',{exact:true}).waitFor();sync='fail';await button(dialog(),'actions.summary').click();await dialog().locator('.summary-operation').getByRole('progressbar').waitFor();
    assert.equal(await dialog().getByRole('alert').count(),0);await button(dialog(),'common.close').click();await button(page.locator('.activity-item'),'activity.viewResult').click();await dialog().locator('.summary-operation').getByRole('progressbar').waitFor();assert.equal(await button(dialog(),'actions.summary').isEnabled(),false);assert.equal(posts.length,1);record('Summary receipt and duplicate guard survive close/reopen after failed refresh without false command error');
  }
  assert.deepEqual(errors,[]);
} finally {
  await fs.writeFile(path.join(reportDir,baseline?'baseline.json':'fixed.json'),JSON.stringify({mode:baseline?'exact-pre-fix':'fixed',results},null,2)+'\n');
  if(releasePost)releasePost();capturedRead?.res.destroy();await browser?.close();for(const stream of streams)stream.end();for(const res of heldReads)res.destroy();server.closeAllConnections();await new Promise(resolve=>server.close(resolve));
}

import {fixtureSession} from './scripts/fixture-session.mjs';
// Native CI proof only. Never point this at an installed browser or user profile.
import {chromium} from 'playwright';
import fs from 'node:fs/promises';
import path from 'node:path';
import http from 'node:http';
import {spawn,execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
import {fixturePanicMonitor} from '../scripts/fixture-panic.mjs';

if(process.env.GITHUB_ACTIONS!=='true'||process.env.RUNNER_OS!=='Windows')throw Error('Run only in the approved isolated Windows repository CI job.');
const root=fileURLToPath(new URL('..',import.meta.url));
const reportPath=path.join(root,'test-results','companion-consent-native.json');
await fs.mkdir(path.dirname(reportPath),{recursive:true});await fs.mkdir(path.join(root,'.cache'),{recursive:true});
const temporary=await fs.mkdtemp(path.join(root,'.cache','companion-consent-native-'));
const report={status:'running',scope:'Bundled Chromium, actual browser-action popup, packaged assets, isolated normal profile, already-held synthetic grants',unverified:['new optional permission prompt acceptance/denial','native permission revocation','private stores','Firefox/containers/FPI','real accounts and websites'],checks:[]};
const save=()=>fs.writeFile(reportPath,JSON.stringify(report,null,2));
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function until(fn,label,ms=15000){const end=Date.now()+ms;while(Date.now()<end){const value=await fn();if(value)return value;await sleep(100);}throw Error('Timed out: '+label);}
let context,fixture,fixtureClosed,bridge;let fixtureExited=false;const panic=fixturePanicMonitor();
const bridgeRequests=[];let releaseRefresh;let delayedRefresh=false;
try{
  const target=path.resolve(root,process.env.CARGO_TARGET_DIR||'target',process.env.CARGO_BUILD_TARGET||'');
  const binary=path.join(target,'debug','examples','dashboard_fixture.exe');await fs.access(binary);
  const portsFile=path.join(temporary,'ports.json');
  fixture=spawn(binary,[],{cwd:root,env:{...process.env,HYCLI_FIXTURE_OUTPUT:portsFile,HYCLI_TEST_DATA:path.join(temporary,'data'),HYCLI_NO_SCREENSHOTS:'1'},stdio:['ignore','ignore','pipe']});
  fixture.stderr.on('data',chunk=>panic.write(chunk));fixture.on('exit',()=>{fixtureExited=true;});
  fixtureClosed=new Promise(resolve=>fixture.once('close',resolve));
  fixture.on('error',error=>{report.fixtureLaunchError=String(error);});
  const ports=await until(async()=>{panic.check();if(fixtureExited||report.fixtureLaunchError)throw Error('Fixture failed before readiness');try{return JSON.parse(await fs.readFile(portsFile,'utf8'));}catch{return null;}},'fixture',120000);
  const {cookie:sessionCookie,csrf}=fixtureSession(ports);
  const api=async(route,body)=>{const response=await fetch(ports.dashboard+route,{method:body?'POST':'GET',headers:{cookie:sessionCookie,'x-hycli-csrf':csrf,'Content-Type':'application/json'},body:body?JSON.stringify(body):undefined});return{status:response.status,body:await response.json()};};
  const downloaded=await fetch(ports.dashboard+'/api/browser/extension',{headers:{cookie:sessionCookie}});assert.equal(downloaded.status,200);
  const zip=path.join(temporary,'companion.zip'),extension=path.join(temporary,'extension');await fs.writeFile(zip,Buffer.from(await downloaded.arrayBuffer()));
  await fs.mkdir(extension);execFileSync(path.join(process.env.SystemRoot,'System32','tar.exe'),['-xf',zip,'-C',extension],{stdio:'pipe'});
  for(const name of ['popup.js','background.js','site-scope.js','vendor/tldts.js','vendor/tldts.LICENSE'])assert.deepEqual(await fs.readFile(path.join(extension,name)),await fs.readFile(path.join(root,'browser-companion',name)),name+' must be the packaged production bytes');
  report.checks.push('Downloaded real Rust companion archive; production scripts and parser/license match byte-for-byte');

  bridge=http.createServer(async(req,res)=>{
    res.setHeader('Access-Control-Allow-Origin',req.headers.origin||'*');res.setHeader('Access-Control-Allow-Headers','content-type');res.setHeader('Access-Control-Allow-Methods','POST, OPTIONS');
    if(req.method==='OPTIONS'){res.writeHead(204);res.end();return;}
    let text='';for await(const chunk of req)text+=chunk;const body=JSON.parse(text||'{}');bridgeRequests.push({route:req.url,body});
    if(req.url==='/bridge/refresh'&&delayedRefresh){await new Promise(resolve=>{releaseRefresh=resolve;});res.writeHead(403);res.end('{}');return;}
    const value=req.url==='/bridge/pair'?{url:site,token:'synthetic-pair',expires_at:Date.now()+60000}:{connected:true,session_token:'synthetic-session'};
    res.setHeader('Content-Type','application/json');res.end(JSON.stringify(value));
  });
  await new Promise(resolve=>bridge.listen(0,'127.0.0.1',resolve));
  const address='http://127.0.0.1:'+bridge.address().port;
  const site='http://www.example.com:'+bridge.address().port;
  const parent='http://example.com:'+bridge.address().port;
  const manifest=JSON.parse(await fs.readFile(path.join(extension,'manifest.json'),'utf8'));
  // Same approved foundation as the raw visibility matrix: required synthetic
  // grants only. This does NOT claim to automate accepting a new host prompt.
  const cookieHosts=['http://www.example.com/*','http://example.com/*'];
  manifest.host_permissions=[...manifest.host_permissions,...cookieHosts];
  await fs.writeFile(path.join(extension,'manifest.json'),JSON.stringify(manifest));
  const seededCookies=[{name:'www_host',value:'synthetic',url:site+'/'},{name:'parent_domain',value:'synthetic',domain:'.example.com',path:'/'},{name:'apex_host',value:'synthetic',url:parent+'/'}];
  // Reproduce the prior native empty result against positively seeded cookies
  // in a separate fresh profile, before testing the corrected host grants.
  const portManifest={...manifest,host_permissions:[...manifest.host_permissions.filter(p=>!cookieHosts.includes(p)),site+'/*',parent+'/*']};
  await fs.writeFile(path.join(extension,'manifest.json'),JSON.stringify(portManifest));
  context=await chromium.launchPersistentContext(path.join(temporary,'port-profile'),{channel:'chromium',headless:false,chromiumSandbox:true,timeout:30000,args:[`--disable-extensions-except=${extension}`,`--load-extension=${extension}`]});
  await context.addCookies(seededCookies);
  assert.deepEqual((await context.cookies()).map(c=>c.name).sort(),['apex_host','parent_domain','www_host']);
  const portWorker=context.serviceWorkers()[0]||await context.waitForEvent('serviceworker',{timeout:15000});
  report.portGrantProbe=await portWorker.evaluate(async site=>({all:(await chrome.cookies.getAll({})).map(c=>c.name),url:(await chrome.cookies.getAll({url:site})).map(c=>c.name)}),site);
  assert.deepEqual(report.portGrantProbe.all,[],'Reproduce prior all-cookie invisibility for port-specific grants');
  // URL-scoped behavior is measured, not assumed; it cannot alone import
  // cookies scoped to paths other than the root selected-site URL.
  await context.close();context=null;
  await fs.writeFile(path.join(extension,'manifest.json'),JSON.stringify(manifest));
  context=await chromium.launchPersistentContext(path.join(temporary,'profile'),{channel:'chromium',headless:false,chromiumSandbox:true,timeout:30000,args:[`--disable-extensions-except=${extension}`,`--load-extension=${extension}`]});
  await context.route('**/*',async route=>{
    const url=new URL(route.request().url());
    if(url.hostname==='127.0.0.1')return route.continue();
    if(['www.example.com','example.com','sibling.example.com'].includes(url.hostname))return route.fulfill({status:200,contentType:'text/html',body:'<!doctype html><title>Owned synthetic website</title><p>Owned synthetic website</p>'});
    return route.abort();
  });
  const worker=context.serviceWorkers()[0]||await context.waitForEvent('serviceworker',{timeout:15000});const id=new URL(worker.url()).hostname;
  report.browser=await worker.evaluate(()=>navigator.userAgent);
  const permissions=await worker.evaluate(()=>chrome.permissions.getAll());
  assert.deepEqual([...(permissions.origins||[])].sort(),[...manifest.host_permissions].sort(),'Only exact synthetic fixture grants are permitted');
  report.grantedOrigins=permissions.origins;
  await context.addCookies(seededCookies);
  assert.deepEqual((await context.cookies()).map(c=>c.name).sort(),['apex_host','parent_domain','www_host'],'Synthetic cookies must exist before extension import');
  report.checks.push('Synthetic cookies exist in the isolated browser before extension import');
  const website=await context.newPage();await website.goto(site);await website.bringToFront();
  const cdp=await context.browser().newBrowserCDPSession();
  let nextId=0;const outstanding=new Map();
  cdp.on('Target.receivedMessageFromTarget',event=>{const message=JSON.parse(event.message);const key=event.sessionId+':'+message.id;const pending=outstanding.get(key);if(pending){outstanding.delete(key);message.error?pending.reject(Error(message.error.message)):pending.resolve(message.result);}});
  async function openPopup(){
    await worker.evaluate(()=>chrome.action.openPopup());
    const target=await until(async()=>{const result=await cdp.send('Target.getTargets');return result.targetInfos.find(t=>t.url===`chrome-extension://${id}/popup.html`);},'actual browser-action popup target');
    const {sessionId}=await cdp.send('Target.attachToTarget',{targetId:target.targetId,flatten:false});
    const send=(method,params={})=>new Promise((resolve,reject)=>{const command=++nextId,key=sessionId+':'+command;const timer=setTimeout(()=>{outstanding.delete(key);reject(Error('Popup protocol timed out: '+method));},15000);outstanding.set(key,{resolve:value=>{clearTimeout(timer);resolve(value);},reject:error=>{clearTimeout(timer);reject(error);}});cdp.send('Target.sendMessageToTarget',{sessionId,message:JSON.stringify({id:command,method,params})}).catch(reject);});
    const evaluate=async expression=>{const r=await send('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw Error('Popup evaluation failed');return r.result.value;};
    const click=async selector=>{const rect=await evaluate(`(async()=>{const e=document.querySelector(${JSON.stringify(selector)});e.scrollIntoView();await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));const r=e.getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2};})()`);await send('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',clickCount:1,...rect});await send('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',clickCount:1,...rect});};
    const fill=async(selector,text)=>{await click(selector);await evaluate(`document.querySelector(${JSON.stringify(selector)}).select()`);await send('Input.insertText',{text});};
    await until(()=>evaluate("document.readyState==='complete'"),'popup ready');
    return{evaluate,click,fill};
  }
  const popup=await openPopup();
  await popup.click('details summary');await popup.fill('#address',address);await popup.fill('#code','SYNTHETIC');await popup.click('#pair');
  await until(()=>popup.evaluate("!document.getElementById('connect-step').hidden"),'pairing');
  assert.equal(await popup.evaluate("document.getElementById('permission-origins').textContent"),cookieHosts.join('\n'));
  const active=await worker.evaluate(async()=>{const tabs=await chrome.tabs.query({active:true,currentWindow:true});return tabs[0]?.url;});assert.equal(new URL(active).origin,site,'Browser-action popup must retain the real website as active tab');
  // Observe real dispatched input without replacing the production handler or
  // browser tab/cookie/permission APIs. Only synthetic fixture metadata is kept.
  await popup.evaluate("document.getElementById('connect').addEventListener('click',e=>{globalThis.nativeClickProof={trusted:e.isTrusted,active:navigator.userActivation.isActive};},{capture:true,once:true})");
  await popup.click('#connect');
  let connected;
  try {
    connected=await until(async()=>{
      const request=bridgeRequests.find(r=>r.route==='/bridge/connect');if(request)return request;
      const error=await popup.evaluate("document.getElementById('status').className==='error'&&document.getElementById('status').textContent");
      if(error)throw Error('Production popup rejected connection: '+error);
      return null;
    },'real popup connection');
  } catch(error) {
    report.popupDiagnostic=await popup.evaluate("({click:globalThis.nativeClickProof||null,status:document.getElementById('status').textContent,disabled:document.getElementById('connect').disabled,visibility:document.visibilityState})").catch(()=>({unavailable:true}));
    report.cookieDiagnostic=await worker.evaluate(async()=>({stores:await chrome.cookies.getAllCookieStores(),cookies:(await chrome.cookies.getAll({})).map(({name,domain,path,hostOnly,secure,storeId,session,expirationDate})=>({name,domain,path,hostOnly,secure,storeId,session,expirationDate}))})).catch(()=>({unavailable:true}));
    throw error;
  }
  assert.deepEqual(connected.body.cookies.map(c=>c.name).sort(),['parent_domain','www_host']);
  assert.ok(!connected.body.identity_source,'No pre-connection identity is fabricated');
  report.checks.push('Actual browser-action popup shows finite scopes and connects through native click with already-held grants; parent-domain cookie imported, apex hostOnly excluded');
  const scope=site+':normal';
  await until(()=>worker.evaluate(async scope=>!!(await chrome.storage.local.get('connection:'+scope))['connection:'+scope],scope),'persisted connection after popup response');
  await website.bringToFront();await website.evaluate(()=>fetch('/items'));
  await until(()=>bridgeRequests.some(r=>r.route==='/bridge/observe'),'positive native observation control');
  report.checks.push('Positive site-origin request reaches the production native observer and synthetic bridge');
  await context.clearCookies();await sleep(3000);
  assert.equal(bridgeRequests.filter(r=>r.route==='/bridge/refresh').length,0,'Empty visibility must not overwrite the saved session');
  assert.equal(await worker.evaluate(async scope=>!!(await chrome.storage.local.get('connection:'+scope))['connection:'+scope],scope),true);
  report.checks.push('Native cookie removal/empty query preserves saved connection without a refresh payload');
  await context.addCookies([{name:'www_host',value:'synthetic',url:site+'/'}]);
  await until(()=>bridgeRequests.some(r=>r.route==='/bridge/refresh'),'positive native cookie-change refresh');
  delayedRefresh=true;releaseRefresh=undefined;
  const handledRefresh=worker.evaluate(async scope=>{const connection=(await chrome.storage.local.get('connection:'+scope))['connection:'+scope];await refreshConnection(scope,connection);return true;},scope);
  void handledRefresh.catch(()=>{}); // Cleanup may close a pending request after an earlier assertion failure.
  await until(()=>releaseRefresh,'pending explicitly awaited refresh');
  await worker.evaluate(async scope=>{const key='connection:'+scope;const connection=(await chrome.storage.local.get(key))[key];await chrome.storage.local.set({[key]:{...connection,token:'synthetic-replacement'}});},scope);
  releaseRefresh();assert.equal(await handledRefresh,true,'Wait for the real production retirement path to finish');
  assert.equal(await worker.evaluate(async scope=>(await chrome.storage.local.get('connection:'+scope))['connection:'+scope]?.token==='synthetic-replacement',scope),true);
  delayedRefresh=false;report.checks.push('Delayed forbidden refresh cannot erase a newly replaced synthetic connection');
  await website.bringToFront();await website.goto('http://sibling.example.com:'+bridge.address().port);
  const wrongTabPopup=await openPopup();await wrongTabPopup.click('details summary');await wrongTabPopup.fill('#address',address);await wrongTabPopup.fill('#code','SYNTHETIC');await wrongTabPopup.click('#pair');
  await until(()=>wrongTabPopup.evaluate("!document.getElementById('connect-step').hidden"),'wrong-tab pairing');await wrongTabPopup.click('#connect');
  await until(()=>wrongTabPopup.evaluate("document.getElementById('status').className==='error'"),'wrong-tab rejection');
  assert.equal(bridgeRequests.filter(r=>r.route==='/bridge/connect').length,1,'Different active website must not replace the paired session');
  report.checks.push('Real action popup rejects a different active website without connecting or replacing the account');
  const observedBeforeParent=bridgeRequests.filter(r=>r.route==='/bridge/observe').length;
  await website.bringToFront();await website.goto(parent);await website.evaluate(()=>fetch('/account'));await sleep(1800);
  assert.equal(bridgeRequests.filter(r=>r.route==='/bridge/observe').length,observedBeforeParent,'Cookie-only ancestor grant must not forward parent request traffic');
  assert.equal(await worker.evaluate(async parent=>!!(await chrome.storage.session.get('identity:'+parent+':normal'))['identity:'+parent+':normal'],parent),false);
  report.checks.push('Parent cookie grant does not record identity or forward native parent request traffic');


  // Independent real Rust validation: browser visibility loss and server auth
  // rejection are distinct. Only this server response should establish expiry.
  const beforeAuth=await api('/api/state');const accountId=beforeAuth.body.accounts.find(a=>a.site_id==='library' && a.verified_identity)?.id;assert.ok(accountId);
  const expiredControl=await fetch(ports.website+'/fixture/control',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({fail_auth:true})});assert.equal(expiredControl.status,200);
  const refreshed=await api('/api/accounts/'+accountId+'/refresh',{});assert.equal(refreshed.status,200);
  const state=await api('/api/state');const account=state.body.accounts.find(a=>a.id===accountId);assert.equal(account.status,'expired');assert.equal(account.verified_identity,false);
  report.checks.push('Real Rust account validation marks synthetic server-rejected authentication expired, separately from empty browser visibility');
  panic.check();report.status='passed';
}catch(error){report.status='failed';report.error=String(error);process.exitCode=1;console.error(error);}
finally{
  // Retain evidence before any teardown can fail, then update it with errors.
  await save();const cleanupErrors=[];
  const cleanup=async(label,operation,timeout=5000)=>{
    let timer;
    try{await Promise.race([Promise.resolve().then(operation),new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('timed out')),timeout);})]);}
    catch(error){cleanupErrors.push(label+': '+String(error));}
    finally{clearTimeout(timer);}
  };
  releaseRefresh?.();
  await cleanup('browser close',()=>context?.close());
  await cleanup('synthetic bridge close',()=>new Promise(resolve=>{if(!bridge)return resolve();bridge.closeAllConnections();bridge.close(resolve);}));
  await cleanup('fixture stop',async()=>{
    if(fixture&&!fixtureExited){fixture.kill();await Promise.race([fixtureClosed,sleep(2000)]);if(!fixtureExited)fixture.kill('SIGKILL');}
    if(fixtureClosed)await fixtureClosed;
  });
  try{panic.check();}catch(error){cleanupErrors.push('fixture panic: '+String(error));}
  await cleanup('owned temporary directory removal',()=>fs.rm(temporary,{recursive:true,force:true}));
  if(cleanupErrors.length){report.status='failed';report.cleanupErrors=cleanupErrors;process.exitCode=1;}
  await save();console.log(JSON.stringify(report,null,2));
  // A broken browser process must not keep CI alive after failed bounded cleanup.
  if(cleanupErrors.length)process.exit(1);
}

import {installFixtureSession} from './scripts/fixture-session.mjs';
// Production Rust dashboard + production UI. Only the remote website and
// browser-reconnect timing are synthetic. No mocked dashboard endpoints.
import {chromium} from 'playwright';
import {spawn} from 'node:child_process';
import fs from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
import {fixturePanicMonitor} from '../scripts/fixture-panic.mjs';
const root=fileURLToPath(new URL('..',import.meta.url));
const en=JSON.parse(await fs.readFile(path.join(root,'dashboard/src/locales/en.json'),'utf8'));
assert.ok(process.env.BROWSER_BIN,'BROWSER_BIN must identify the installed, sandbox-enabled Chrome');
const target=path.resolve(root,process.env.CARGO_TARGET_DIR||'target');
const native=process.env.CARGO_BUILD_TARGET?path.join(target,process.env.CARGO_BUILD_TARGET):target;
const fixture=process.env.HYCLI_BINDING_FIXTURE_BIN||path.join(native,'debug/examples/binding_dashboard_fixture'+(process.platform==='win32'?'.exe':''));
const scratch=path.resolve(process.env.HYCLI_TEST_DATA||path.join(root,'.cache'));
await fs.mkdir(scratch,{recursive:true});const directory=await fs.mkdtemp(path.join(scratch,'binding-ui-'));const output=path.join(directory,'ports.json');
const panic=fixturePanicMonitor();let exited=false,launchError,browser,context;
const server=spawn(fixture,[],{cwd:root,env:{...process.env,HYCLI_TEST_DATA:path.join(directory,'data'),HYCLI_FIXTURE_OUTPUT:output},stdio:['ignore','inherit','pipe']});
server.stderr.on('data',chunk=>{process.stderr.write(chunk);panic.write(chunk);});server.on('exit',()=>{exited=true;});server.on('error',error=>{launchError=error;});const closed=new Promise(resolve=>server.once('close',resolve));
const poll=async(fn,timeout=30000)=>{const until=Date.now()+timeout;while(Date.now()<until){panic.check();if(launchError)throw launchError;if(exited)throw Error('Fixture exited');if(await fn())return;await new Promise(resolve=>setTimeout(resolve,100));}throw Error('Fixture condition timed out');};
try {
 let ports;await poll(async()=>{try{ports=JSON.parse(await fs.readFile(output,'utf8'));return true;}catch{return false;}},120000);
 const status=()=>fetch(ports.website+'/fixture/status').then(response=>response.json());
 const control=async action=>{const response=await fetch(ports.website+'/fixture/control',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({action})});assert.equal(response.status,200);};
 browser=await chromium.launch({headless:true,chromiumSandbox:true,executablePath:process.env.BROWSER_BIN});
 context=await browser.newContext({viewport:{width:1440,height:1000}});
 const allowed=new Set([ports.dashboard,ports.website]);await context.route('**/*',route=>allowed.has(new URL(route.request().url()).origin)?route.continue():route.abort());
 const errors=[];const page=await context.newPage();page.on('pageerror',error=>errors.push(error.message));page.setDefaultTimeout(60000);
 await installFixtureSession(context, ports);
 await page.goto(ports.dashboard);await page.getByRole('heading',{name:en['sites.title'],exact:true}).waitFor();
 const button=(scope,key)=>scope.getByRole('button',{name:en[key],exact:true});const dialog=()=>page.getByRole('dialog');
 const open=async()=>{await button(page,'nav.websites').click();await button(page.locator('.site-card').filter({hasText:'Named workspace'}),'sites.openActions').click();};
 const close=()=>button(dialog(),'common.close').click();
 const select=async id=>{await open();const saved=page.waitForResponse(response=>new URL(response.url()).pathname==='/api/sites/named-workspace'&&response.request().method()==='PATCH');await dialog().getByLabel(en['accounts.choose'],{exact:true}).selectOption(id);assert.equal((await saved).status(),200);await poll(async()=>(await status()).selected===id);await close();};
 const run=async(expected,dispatch=true)=>{const before=(await status()).requests.length;await open();await dialog().locator('.action-row').filter({hasText:'Read session'}).click();await button(dialog(),'actions.run').click();if(dispatch){await dialog().getByText(expected,{exact:true}).waitFor();assert.deepEqual((await status()).requests.slice(before),[expected]);}else{await dialog().getByText(en['errors.auth_required'],{exact:false}).waitFor();assert.equal((await status()).requests.length,before);}await close();};
 const original=await status();assert.notEqual(original.source_site,'named-workspace');assert.equal(original.selected,'');
 await run('Anonymous');
 await select(ports.account_id);await run('Alpha');assert.equal((await status()).source_site,original.source_site,'Selection cannot move source ownership');
 await select('');await run('Anonymous');await select(ports.account_id);
 // A real UI refresh starts with A. A synthetic source reconnect changes the
 // same account's credential to B. Release the stale response before refreshing
 // B: the production network layer serializes requests for one account.
 await button(page,'nav.accounts').click();await control('hold');
 const stale=page.waitForResponse(response=>new URL(response.url()).pathname===`/api/accounts/${ports.account_id}/refresh`);
 await button(page.locator('.account-card').filter({hasText:'Imported Alpha'}),'accounts.refresh').click();await poll(async()=>(await status()).entered);
 await control('reconnect');
 await control('release');assert.equal((await stale).status(),409,'Stale A refresh must conflict after reconnect');
 const second=await context.newPage();second.setDefaultTimeout(60000);await second.goto(ports.dashboard);await button(second,'nav.accounts').click();
 const fresh=second.waitForResponse(response=>new URL(response.url()).pathname===`/api/accounts/${ports.account_id}/refresh`);
 await button(second.locator('.account-card').filter({hasText:'Imported Alpha'}),'accounts.refresh').click();assert.equal((await fresh).status(),200);await poll(async()=>(await status()).identity==='Beta');
 await second.close();
 assert.equal((await status()).identity,'Beta');await run('',false);
 // Re-consent is an actual user choice, not automatic resume from stale account_id.
 await select('');await run('Anonymous');await select(ports.account_id);await run('Beta');
 const state=await page.evaluate(async()=>JSON.stringify(await(await fetch('/api/state')).json()));assert.ok(!state.includes('synthetic-alpha')&&!state.includes('synthetic-beta'));assert.deepEqual(errors,[]);panic.check();
 console.log('PASS: real dashboard explicit binding, anonymous choice, source ownership, stale refresh and exact HTTP authority.');
} finally {
 await context?.close();await browser?.close();if(!exited){server.kill('SIGTERM');await Promise.race([closed,new Promise(resolve=>setTimeout(resolve,5000))]);if(!exited)server.kill('SIGKILL');}await closed;panic.check();
}

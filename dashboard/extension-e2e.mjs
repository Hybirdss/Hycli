import {fileURLToPath} from 'node:url';
import {chromium} from 'playwright';
import fs from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
process.chdir(fileURLToPath(new URL('..',import.meta.url)));
const ports=JSON.parse(await fs.readFile(process.env.HYCLI_FIXTURE_OUTPUT || '.cache/e2e-ports.json','utf8'));
const root=path.resolve(process.env.HYCLI_TEST_DATA || '.cache','hycli-extension-test-'+crypto.randomUUID());
const extension=path.join(root,'extension');await fs.mkdir(extension,{recursive:true});
for(const name of ['manifest.json','popup.html','popup.js','background.js','style.css','locales.js'])await fs.copyFile('browser-companion/'+name,path.join(extension,name));
await fs.copyFile('dashboard/public/brand/browser-companion.png',path.join(extension,'bird.png'));
const browser=await chromium.launchPersistentContext(path.join(root,'profile'),{headless:true,channel:process.env.BROWSER_BIN?undefined:'chromium',executablePath:process.env.BROWSER_BIN,args:['--no-sandbox','--disable-gpu','--disable-dev-shm-usage',`--disable-extensions-except=${extension}`,`--load-extension=${extension}`],viewport:{width:480,height:900},timeout:120000});
browser.setDefaultTimeout(120000);
try {
 console.log('Isolated browser started.');
 const worker=browser.serviceWorkers()[0]||await browser.waitForEvent('serviceworker',{timeout:60000});const id=new URL(worker.url()).host;
 const session=await fetch(ports.dashboard+'/api/session');const cookie=session.headers.get('set-cookie').split(';')[0];const csrf=(await session.json()).csrf;
 const request=async(route,body)=>{const r=await fetch(ports.dashboard+route,{method:body?'POST':'GET',headers:{cookie,'x-hycli-csrf':csrf,'Content-Type':'application/json'},body:body?JSON.stringify(body):undefined});assert.equal(r.status,200);return r.json();};
 console.log('Companion service worker ready.');
 const pair=await request('/api/accounts/pair',{site_id:'library',url:ports.website});
 await browser.addCookies([{name:'session',value:'synthetic-browser-secret',url:ports.website,httpOnly:true,sameSite:'Lax'}]);
 const website=await browser.newPage();await website.goto(ports.website,{waitUntil:'domcontentloaded',timeout:120000});
 const popup=await browser.newPage();const errors=[];popup.on('pageerror',e=>errors.push(e.message));await popup.goto(`chrome-extension://${id}/popup.html`,{waitUntil:'domcontentloaded',timeout:120000});
 console.log('Pairing from the real extension popup.');
 await popup.locator('details summary').click();
 await popup.locator('#address').fill(ports.dashboard);await popup.locator('#code').fill(pair.code);await popup.locator('#pair').click();await popup.locator('#connect-step').waitFor({state:'visible'});
 await popup.locator('#profile').fill('Synthetic test profile');await popup.locator('#connect').click();await popup.locator('#status').getByText('Account connected',{exact:false}).waitFor({timeout:60000});
 await popup.screenshot({animations:'disabled',path:'docs/design/browser-companion-actual.png',timeout:120000});
 console.log('Cookie transfer completed; verifying observed identity.');
 await website.bringToFront();await website.evaluate(async()=>{await fetch('/account');await fetch('/articles?q=fixture-query-must-not-be-recorded');});
 const start=Date.now();let state;while(Date.now()-start<45000){state=await request('/api/state');if(state.accounts.some(a=>a.profile==='Synthetic test profile'&&a.verified_identity))break;await new Promise(r=>setTimeout(r,500));}
 const account=state.accounts.find(a=>a.profile==='Synthetic test profile');assert.ok(account);assert.equal(account.verified_identity,true);assert.equal(account.email,'mina@example.com');
 const stateText=JSON.stringify(state);assert.ok(!stateText.includes('synthetic-browser-secret'));assert.ok(!stateText.includes('fixture-query-must-not-be-recorded'));
 console.log('Checking automatic expiry and renewed sign-in in the same profile.');
 await browser.clearCookies();
 let refreshed;for(let i=0;i<90;i++){refreshed=(await request('/api/state')).accounts.find(a=>a.id===account.id);if(refreshed?.status==='expired')break;await new Promise(r=>setTimeout(r,500));}
 assert.equal(refreshed.status,'expired');assert.equal(refreshed.verified_identity,false);
 await browser.addCookies([{name:'session',value:'synthetic-browser-secret',url:ports.website,httpOnly:true,sameSite:'Lax'}]);
 for(let i=0;i<90;i++){refreshed=(await request('/api/state')).accounts.find(a=>a.id===account.id);if(refreshed?.verified_identity&&refreshed.status==='connected')break;await new Promise(r=>setTimeout(r,500));}
 assert.equal(refreshed.status,'connected');assert.equal(refreshed.verified_identity,true);assert.equal(refreshed.email,'mina@example.com');
 assert.deepEqual(errors,[]);console.log(JSON.stringify({extension:'passed',cookieTransfer:'isolated synthetic profile only',identity:'verified from actual fixture response',refresh:'logout expires; renewed sign-in reconnects automatically',privateValues:'not exported',profileDirectory:root},null,2));
}finally{await browser.close();}

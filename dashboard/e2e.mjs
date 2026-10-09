import {fileURLToPath} from 'node:url';
import {chromium} from 'playwright';
import fs from 'node:fs/promises';
import assert from 'node:assert/strict';
process.chdir(fileURLToPath(new URL('..', import.meta.url)));
const ports=JSON.parse(await fs.readFile(process.env.HYCLI_FIXTURE_OUTPUT || '.cache/e2e-ports.json','utf8'));
const en=JSON.parse(await fs.readFile('dashboard/src/locales/en.json','utf8'));
const browser=process.env.TEST_CDP?await chromium.connectOverCDP(process.env.TEST_CDP):await chromium.launch({headless:true,executablePath:process.env.BROWSER_BIN,args:['--no-sandbox','--disable-gpu','--disable-dev-shm-usage']});
const context=await browser.newContext({viewport:{width:1536,height:1024},deviceScaleFactor:1});
const page=await context.newPage();page.setDefaultTimeout(120000);const consoleErrors=[];page.on('pageerror',e=>consoleErrors.push(e.message));
const poll=async(fn,timeout=90000)=>{const start=Date.now();while(Date.now()-start<timeout){if(await fn())return;await new Promise(r=>setTimeout(r,500));}throw Error('Condition did not become true');};
const fixtureStatus=()=>fetch(ports.website+'/fixture/status').then(r=>r.json());
const button=(parent,key)=>parent.getByRole('button',{name:en[key],exact:true});
const dialog=()=>page.getByRole('dialog');
const screenshot=async options=>{if(process.env.HYCLI_NO_SCREENSHOTS==='1')return;await page.locator('img').evaluateAll(images=>Promise.all(images.map(image=>image.decode().catch(()=>{}))));return page.screenshot({animations:'disabled',...options});};
const openLibrary=()=>button(page.locator('.site-card').filter({hasText:'Research library'}).first(),'sites.openActions').click();
try {
 const baseline=(await fixtureStatus()).writes;
 console.log('Opening the Rust dashboard.');
 await page.goto(ports.dashboard,{waitUntil:'domcontentloaded',timeout:120000});await page.getByRole('heading',{name:en['sites.title'],exact:true}).waitFor();
 await fs.mkdir('docs/images',{recursive:true});await screenshot({path:'docs/images/dashboard.png',fullPage:true,timeout:120000});
 console.log('Checking the provider settings flow.');
 await button(page,'nav.connections').click();await button(page.locator('.chatgpt-method').filter({hasText:en['connections.apiHint']}),'common.edit').click();await dialog().getByLabel(en['connections.key'],{exact:false}).fill('synthetic-provider-key');await dialog().getByLabel(en['connections.model'],{exact:false}).fill('fixture-model');await button(dialog(),'common.save').click();await dialog().waitFor({state:'hidden'});
 const publicState=await page.evaluate(async()=>JSON.stringify(await (await fetch('/api/state')).json()));assert.ok(!publicState.includes('synthetic-provider-key'));await screenshot({path:'docs/images/ai-connections.png',fullPage:true,timeout:120000});await button(page,'nav.websites').click();
 console.log('Dashboard ready; testing a read.');
 await openLibrary();await dialog().getByRole('button',{name:/^Find articles/}).click();await dialog().getByLabel('Search for',{exact:false}).fill('observe');await button(dialog(),'actions.run').click();await dialog().getByText('Learning to observe',{exact:true}).waitFor();assert.equal((await fixtureStatus()).writes,baseline);
 console.log('Read completed; testing the result summary.');
 await button(dialog(),'actions.summary').click();await dialog().getByText('Two references are ready to use:',{exact:false}).waitFor();await button(dialog(),'common.close').click();
 await openLibrary();await dialog().getByRole('button',{name:/^Create a task/}).click();await dialog().getByLabel('Task name',{exact:false}).fill('A task approved in the fixture');await button(dialog(),'actions.review').click();await page.getByRole('heading',{name:en['safety.changeTitle'],exact:true}).waitFor();assert.equal((await fixtureStatus()).writes,baseline,'Opening review must not send a write');
 await button(dialog(),'common.cancel').click();assert.equal((await fixtureStatus()).writes,baseline,'Cancelling must not send a write');
 await openLibrary();await dialog().getByRole('button',{name:/^Create a task/}).click();await dialog().getByLabel('Task name',{exact:false}).fill('A task approved in the fixture');await button(dialog(),'actions.review').click();await button(dialog(),'safety.allowOnce').click();await dialog().locator('.result-heading').getByText(en['activity.completed'],{exact:true}).waitFor();await dialog().getByText('A task approved in the fixture',{exact:true}).waitFor();assert.equal((await fixtureStatus()).writes,baseline+1);await screenshot({path:'docs/images/action-result.png',timeout:120000});await button(dialog(),'common.close').click();
 console.log('Approval cancellation and single-use execution passed.');
 // Preparation uses the actual page -> linked OpenAPI -> mock AI loop. It must never test writes.
 const previousJobs=await page.evaluate(async()=>{const r=await fetch('/api/state');return (await r.json()).jobs.map(j=>j.id);});
 await page.getByLabel(en['sites.urlLabel'],{exact:true}).fill(ports.website);await page.locator('.prepare-intent summary').click();await page.getByLabel(en['sites.intentLabel'],{exact:true}).fill('Find references about design');await button(page,'sites.connect').click();await poll(async()=>{const jobs=await page.evaluate(async()=>{const r=await fetch('/api/state');return (await r.json()).jobs;});const job=jobs.find(j=>j.kind==='prepare'&&!previousJobs.includes(j.id));if(job?.status==='failed')throw Error('Preparation failed: '+job.error_code);if(job?.status==='completed'){assert.equal(job.intent,'Find references about design');assert.equal(job.agents.length,3);assert.ok(job.notes.some(note=>note.code==='environment_checked'));assert.ok(job.notes.some(note=>note.code==='agent_retry'),'Failed candidate read must reach autonomous repair');const failure=job.notes.find(note=>note.code==='tool_failed');assert.ok(failure);assert.ok(job.notes.some(note=>note.code==='tool_completed'&&note.seq>failure.seq));const result=await page.evaluate(async id=>(await fetch('/api/jobs/'+id+'/result')).json(),job.id);assert.ok(result?.verification?.verified_reads>0);assert.ok(job.agents.every(agent=>agent.status==='completed'&&agent.found===1));return true;}return false;},Number(process.env.HYCLI_PREPARATION_TEST_TIMEOUT_MS)||120000);assert.equal((await fixtureStatus()).writes,baseline+1);assert.ok((await fixtureStatus()).intent_prompts>=3,'Preparation intent reaches all workers');
 console.log('Preparation completed.');
 await button(page,'nav.accounts').click();await page.getByRole('heading',{name:en['accounts.title'],exact:true}).waitFor();await screenshot({path:'docs/images/accounts.png',fullPage:true,timeout:120000});
 await button(page,'accounts.add').first().click();await screenshot({path:'docs/images/connect-account.png',timeout:120000});await button(dialog(),'common.close').click();
 // The UI runs against real APIs at narrow and RTL layouts as well.
 await page.setViewportSize({width:390,height:844});await screenshot({path:'docs/design/accounts-mobile.png',fullPage:true,timeout:120000});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),true,'Mobile layout overflows');
 await page.setViewportSize({width:1536,height:1024});await page.getByLabel(en['settings.language'],{exact:true}).first().selectOption('ar');await poll(()=>page.locator('html').getAttribute('dir').then(v=>v==='rtl'));await screenshot({path:'docs/design/accounts-arabic.png',fullPage:true,timeout:120000});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),true,'RTL layout overflows');
 const ar=JSON.parse(await fs.readFile('dashboard/src/locales/ar.json','utf8'));await page.getByLabel(ar['settings.language'],{exact:true}).first().selectOption('en');
 assert.equal((await fixtureStatus()).secret_prompts,0,'A credential entered a model prompt');assert.deepEqual(consoleErrors,[]);
 console.log(JSON.stringify({ui:'passed',provider:'saved and checked without exporting the key',approval:'cancel=0; approve=1',preparation:'no test writes',secret_prompts:0,mobile:'no horizontal overflow',rtl:'no horizontal overflow',screenshots:process.env.HYCLI_NO_SCREENSHOTS==='1'?'skipped; checked separately':'docs/images and docs/design'},null,2));
} catch(error){await screenshot({path:'docs/design/e2e-failure.png',fullPage:true,timeout:120000}).catch(()=>{});console.error(error);process.exitCode=1;} finally{await context.close();if(!process.env.TEST_CDP)await browser.close();}
if(process.env.TEST_CDP)process.exit(process.exitCode||0);

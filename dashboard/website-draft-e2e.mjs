// Production dashboard against an isolated synthetic API; no live sites/accounts/providers.
// Build first, then BROWSER_BIN=/path/to/chromium node dashboard/website-draft-e2e.mjs.
import { chromium } from 'playwright';
import { createServer } from 'node:http';
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';
const root = fileURLToPath(new URL('..', import.meta.url));
const en = JSON.parse(await fs.readFile(path.join(root, 'dashboard/src/locales/en.json'), 'utf8'));
const state = {settings:{locale:'en',theme:'light',default_provider:'fixture'},providers:[{id:'fixture',name:'Fixture AI',connected:true},{id:'other',name:'Other AI',connected:true}],sites:[],accounts:[],jobs:[],approvals:[],engine:{available:true,compatible:true,version:'1'},version:'test'};
const posts = [], streams = new Set();
let pending = null;
const server = createServer(async (req,res) => {
  if (req.url === '/api/events') {res.writeHead(200,{'Content-Type':'text/event-stream'});res.write(': ready\n\n');streams.add(res);req.on('close',()=>streams.delete(res));return;}
  if (req.url.startsWith('/api/')) {
    let body='';for await (const chunk of req) body+=chunk;
    let result;
    if (req.url.startsWith('/api/session')) result={csrf:'synthetic'};
    else if (req.url === '/api/state') result=state;
    else if (req.url === '/api/sites/prepare' && req.method === 'POST') {
      posts.push(JSON.parse(body));
      assert.equal(pending,null,'Only one fixture POST may be pending');
      const status=await new Promise(resolve=>{pending=resolve;});pending=null;
      if(status!==200){res.writeHead(status,{'Content-Type':'application/json'});res.end(JSON.stringify({error:{code:'ai_unavailable'}}));return;}
      state.version=`accepted-${posts.length}`;
      result={job:{id:`prepare-${posts.length}`,kind:'prepare',status:'running'}};
    } else {res.writeHead(404);res.end();return;}
    res.writeHead(200,{'Content-Type':'application/json'});res.end(JSON.stringify(result));return;
  }
  const filename=path.join(root,'web',req.url==='/'?'index.html':decodeURIComponent(req.url.split('?')[0]));
  try {const bytes=await fs.readFile(filename);res.writeHead(200,{'Content-Type':filename.endsWith('.js')?'text/javascript':filename.endsWith('.css')?'text/css':filename.endsWith('.png')?'image/png':'text/html'});res.end(bytes);}catch{res.writeHead(404);res.end();}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
let browser;
try {
  browser=await chromium.launch({headless:true,chromiumSandbox:true,executablePath:process.env.BROWSER_BIN});
  const context=await browser.newContext();
  const origin=`http://127.0.0.1:${server.address().port}`;
  await context.route('**/*',route=>new URL(route.request().url()).origin===origin?route.continue():route.abort());
  const page=await context.newPage();page.setDefaultTimeout(10000);
  const errors=[];page.on('pageerror',error=>errors.push(error.message));
  const url=()=>page.getByRole('textbox',{name:en['sites.urlLabel'],exact:true});
  const intent=()=>page.getByRole('textbox',{name:en['sites.intentLabel'],exact:true});
  const provider=()=>page.getByRole('combobox',{name:en['sites.providerLabel'],exact:true});
  const connect=()=>page.getByRole('button',{name:en['sites.connect'],exact:true});
  const nav=key=>page.locator('.sidebar').getByRole('button',{name:en[key],exact:true});
  const wait=async predicate=>{const until=Date.now()+10000;while(!await predicate()){assert.ok(Date.now()<until,'Fixture condition timed out');await new Promise(resolve=>setTimeout(resolve,10));}};
  const baseline={url:'https://first.example.test',intent:'Read the first report',provider:'fixture'};
  async function fill(draft){await url().fill(draft.url);await intent().fill(draft.intent);await provider().selectOption(draft.provider);}
  async function check(draft){assert.equal(await url().inputValue(),draft.url);assert.equal(await intent().inputValue(),draft.intent);assert.equal(await provider().inputValue(),draft.provider);}
  async function start(){await fill(baseline);const count=posts.length;await connect().click();await wait(()=>pending!==null);assert.equal(posts.length,count+1);assert.deepEqual(posts.at(-1),{...baseline,url:baseline.url+'/'});assert.equal(await connect().isDisabled(),true);assert.equal(await url().isEditable(),true);assert.equal(await intent().isEditable(),true);}
  async function finish(status=200){const response=page.waitForResponse(r=>new URL(r.url()).pathname==='/api/sites/prepare');pending(status);assert.equal((await response).status(),status);await wait(async()=>await connect().isEnabled());}
  async function finishAfterNavigation(){
    const version=`accepted-${posts.length}`;
    const response=page.waitForResponse(r=>new URL(r.url()).pathname==='/api/sites/prepare');pending(200);assert.equal((await response).status(),200);
    // This marker only changes in the accepted POST, so seeing it rendered proves
    // the following snapshot refresh reached React. Allow its continuation to paint.
    await page.locator('.local-status').getByText(`v${version}`,{exact:true}).waitFor();
    await page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve()))));
  }
  await page.goto(origin);await url().waitFor();
  await start();await finish();await check({url:'',intent:'',provider:'fixture'});
  console.log('PASS: unchanged successful submission clears the original draft');
  for (const [label,change] of [['URL',{url:'https://next.example.test'}],['intent',{intent:'Keep this new intent'}],['provider',{provider:'other'}]]) {
    await start();const next={...baseline,...change};
    if(change.url)await url().fill(change.url);if(change.intent)await intent().fill(change.intent);if(change.provider)await provider().selectOption(change.provider);
    await finish();await check(next);console.log(`PASS: editing ${label} preserves the entire newer draft, including unchanged fields`);
  }
  await start();await intent().fill('Temporary edit');await intent().fill(baseline.intent);await finish();await check(baseline);
  console.log('PASS: edit then revert still preserves the user-owned draft');
  await start();await finish(503);await check(baseline);await page.getByRole('alert').getByText(en['errors.ai_unavailable'],{exact:true}).waitFor();
  console.log('PASS: failed submission preserves original values and shows the failure');
  await start();await url().fill('https://retry.example.test');await finish(503);await check({...baseline,url:'https://retry.example.test'});
  console.log('PASS: failed submission preserves edits made while pending');
  await start();await nav('nav.activity').click();await page.getByRole('heading',{name:en['activity.title'],exact:true}).waitFor();
  await finishAfterNavigation();
  await page.getByRole('heading',{name:en['activity.title'],exact:true}).waitFor();assert.equal(new URL(page.url()).hash,'#activity');assert.equal(await page.getByRole('dialog').count(),0);
  await nav('nav.websites').click();await start();await nav('nav.activity').click();await nav('nav.websites').click();const reopened={url:'https://reopened.example.test',intent:'Keep reopened draft',provider:'other'};await fill(reopened);
  await finishAfterNavigation();await check(reopened);
  assert.deepEqual(errors,[]);console.log('PASS: late response cannot navigate away or erase a reopened form draft');
} finally {
  if(pending)pending(503);
  await browser?.close();for(const stream of streams)stream.end();server.closeAllConnections();await new Promise(resolve=>server.close(resolve));
}

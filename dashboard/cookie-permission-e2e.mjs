// Repository CI only. Never use a user's profile, browser binary, or account.
import {chromium} from 'playwright';
import fs from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
import {verifyVisibility} from './cookie-permission-contract.mjs';

if (process.env.GITHUB_ACTIONS !== 'true' || process.env.RUNNER_OS !== 'Windows') {
  throw new Error('This native proof is restricted to the approved Windows repository CI job. Do not run it on a managed desktop.');
}
const root = fileURLToPath(new URL('..', import.meta.url));
const output = path.join(root, 'test-results', 'cookie-permission-report.json');
await fs.mkdir(path.dirname(output), {recursive:true});
await fs.mkdir(path.join(root, '.cache'), {recursive:true});
const temporary = await fs.mkdtemp(path.join(root, '.cache', 'cookie-permission-owned-'));
const report = {status:'running',platform:process.platform,scope:'Bundled Chromium, normal store, required manifest grants only',unverified:['optional grant prompts/denial/revocation','private stores','Firefox/containers/FPI','Chrome-brand/Edge/Brave/Safari','real login/import behavior'],cases:[]};
const save = () => fs.writeFile(output, JSON.stringify(report,null,2));
const summarize = cookies => cookies.map(({name,domain,hostOnly,storeId,partitionKey}) => ({name,domain,hostOnly,storeId,partitionKey}));
try {
  for (const [label, origins] of Object.entries({none:[],www:['https://www.hycli-proof.test/*'],both:['https://www.hycli-proof.test/*','https://hycli-proof.test/*']})) {
    const entry = {label,status:'starting',requestedOrigins:origins};
    report.cases.push(entry); await save();
    const extension = path.join(temporary,'extension-'+label);
    await fs.mkdir(extension);
    await fs.writeFile(path.join(extension,'manifest.json'),JSON.stringify({manifest_version:3,name:'Synthetic cookie visibility '+label,version:'1.0',permissions:['cookies'],host_permissions:origins,background:{service_worker:'worker.js'}}));
    await fs.writeFile(path.join(extension,'worker.js'),'chrome.runtime.onInstalled.addListener(() => {});');
    let context;
    try {
      context = await chromium.launchPersistentContext(path.join(temporary,'profile-'+label),{
        channel:'chromium',headless:true,chromiumSandbox:true,timeout:30000,
        args:[`--disable-extensions-except=${extension}`,`--load-extension=${extension}`],
      });
      // No website is visited. All fixtures are injected into this new owned
      // profile. Block page requests as an additional guard against navigation.
      await context.route('**/*',route => route.abort());
      const worker = context.serviceWorkers()[0] || await context.waitForEvent('serviceworker',{timeout:15000});
      entry.browser = await worker.evaluate(() => navigator.userAgent);
      await context.addCookies([
        {name:'www_host',value:'synthetic',url:'https://www.hycli-proof.test/',secure:true},
        {name:'apex_host',value:'synthetic',url:'https://hycli-proof.test/',secure:true},
        {name:'parent_domain',value:'synthetic',domain:'.hycli-proof.test',path:'/',secure:true},
        {name:'www_domain',value:'synthetic',domain:'.www.hycli-proof.test',path:'/',secure:true},
        {name:'unrelated',value:'synthetic',url:'https://other-proof.test/',secure:true},
        {name:'partitioned_parent',value:'synthetic',domain:'.hycli-proof.test',path:'/',secure:true,sameSite:'None',partitionKey:'https://top-proof.test'},
      ]);
      entry.seeded = summarize(await context.cookies());
      Object.assign(entry, await worker.evaluate(async () => {
        const clean = cookies => cookies.map(({name,domain,hostOnly,storeId,partitionKey}) => ({name,domain,hostOnly,storeId,partitionKey}));
        const stores = await chrome.cookies.getAllCookieStores();
        const out = {permissions:await chrome.permissions.getAll(),stores};
        if (stores.length !== 1) throw new Error('Expected exactly one isolated normal cookie store');
        for (const [key,query] of Object.entries({all:{},www:{url:'https://www.hycli-proof.test/'},store:{storeId:stores[0].id},partitioned:{partitionKey:{topLevelSite:'https://top-proof.test'}}})) out[key] = clean(await chrome.cookies.getAll(query));
        return out;
      }));
      assert.deepEqual([...(entry.permissions.origins || [])].sort(), [...origins].sort(), 'Only the exact fixture origins may be granted');
      entry.observation = verifyVisibility(label,entry);
      entry.status = 'passed';
    } catch (error) {
      entry.status = 'failed'; entry.error = String(error); throw error;
    } finally { await context?.close(); await save(); }
  }
  report.status = 'passed';
  console.log(JSON.stringify(report,null,2));
} catch (error) {
  report.status = 'failed'; report.error = String(error); process.exitCode = 1;
  console.error(error);
} finally {
  await save();
  // Delete only the directory created by this invocation; never export profiles.
  await fs.rm(temporary,{recursive:true,force:true});
}

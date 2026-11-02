import {fixtureSession} from './scripts/fixture-session.mjs';
import {spawn} from 'node:child_process';
import fs from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
const root=fileURLToPath(new URL('..',import.meta.url));
process.chdir(root);
const ports=JSON.parse(await fs.readFile(process.env.HYCLI_FIXTURE_OUTPUT || '.cache/e2e-ports.json','utf8'));
const target=path.resolve(root,process.env.CARGO_TARGET_DIR||'target');
const binary=process.env.HYCLI_BIN||path.join(target,...(process.env.CARGO_BUILD_TARGET?[process.env.CARGO_BUILD_TARGET]:[]),'debug',`hycli${process.platform==='win32'?'.exe':''}`);
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function connect(args){
 const child=spawn(binary,args,{env:{...process.env,HYCLI_DATA_DIR:ports.data},stdio:['pipe','pipe','pipe']});
 let sequence=0,buffer='',errors='';const pending=new Map(),notifications=[];
 child.stderr.on('data',chunk=>{errors+=chunk;});
 const rejectPending=error=>{for(const item of pending.values()){clearTimeout(item.timer);item.reject(error);}pending.clear();};
 child.on('error',rejectPending);child.on('exit',()=>rejectPending(Error(`MCP exited: ${errors.slice(0,500)}`)));
 child.stdout.on('data',chunk=>{buffer+=chunk;while(buffer.includes('\n')){const i=buffer.indexOf('\n'),line=buffer.slice(0,i);buffer=buffer.slice(i+1);if(!line.trim())continue;let message;try{message=JSON.parse(line);}catch(error){rejectPending(error);return;}if(message.id!==undefined&&pending.has(message.id)){const {resolve,reject,timer}=pending.get(message.id);clearTimeout(timer);pending.delete(message.id);if(message.error)reject(Error(message.error.message));else resolve(message.result);}else if(message.method){notifications.push(message);}}});
 const rpc=(method,params)=>new Promise((resolve,reject)=>{const id=++sequence;const timer=setTimeout(()=>{pending.delete(id);reject(Error(`MCP timeout: ${method}`));},120000);pending.set(id,{resolve,reject,timer});child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n');});
 const call=async(name,args)=>{const result=await rpc('tools/call',{name,arguments:args});assert.ok(!result.isError,JSON.stringify(result));return JSON.parse(result.content.find(c=>c.type==='text').text);};
 const initialized=await rpc('initialize',{protocolVersion:'2025-03-26',capabilities:{},clientInfo:{name:'hycli-synthetic-test',version:'1'}});
 child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');
 return {rpc,call,notifications,initialized,close(){child.stdin.end();child.kill('SIGTERM');for(const item of pending.values())clearTimeout(item.timer);if(errors.trim())console.error(errors.slice(0,1500));}};
}
const count=async()=>(await (await fetch(ports.website+'/fixture/status')).json()).writes;
// A plain local request cannot mint a dashboard session; the synthetic fixture has already redeemed its one-time ticket.
assert.equal((await fetch(ports.dashboard+'/api/session')).status,401);
const {cookie,csrf}=fixtureSession(ports);
const auth={cookie,'x-hycli-csrf':csrf};
const allowChanges=writes=>fetch(ports.dashboard+'/api/sites/library',{method:'PATCH',headers:{...auth,'content-type':'application/json'},body:JSON.stringify({writes})});
// Earlier suites may have allowed changes; this one starts from the read-only default.
assert.equal((await allowChanges(false)).status,200);
const scoped=await connect(['mcp','--sites-only','--only-site','library','--allow-local']);
try{
 const tools=(await scoped.rpc('tools/list',{})).tools;
 assert.ok(tools.some(t=>t.name==='hycli_result'));assert.ok(tools.every(t=>t.name.startsWith('library_')||['hycli_result','hycli_check','hycli_activity'].includes(t.name)));assert.ok(!tools.some(t=>/approve|credential|cookie/.test(t.name)));
 const read=await scoped.call('library_find-articles',{q:'fixture'});assert.equal(read.status,200);assert.ok(!JSON.stringify(read).includes('synthetic-browser-secret'));assert.ok(!JSON.stringify(read).includes('synthetic-response-secret'));
 const activity=await scoped.call('hycli_activity',{});assert.ok(activity.every(job=>job.site_id==='library'));assert.ok(activity.some(job=>job.status==='completed'&&job.notes?.length>0));
 assert.ok(!tools.some(t=>t.name==='library_create-task'),'A read-only website offers no change tool');
 const refused=await scoped.rpc('tools/call',{name:'library_create-task',arguments:{title:'MCP fixture task'}});assert.ok(refused.isError);assert.match(refused.content[0].text,/writes_disabled/);
 const check=await scoped.call('hycli_check',{});assert.equal(check[0].site,'library');assert.ok(['ok','no_checks'].includes(check[0].verdict),JSON.stringify(check));
 assert.equal((await allowChanges(true)).status,200);
 const before=await count();const change=await scoped.call('library_create-task',{title:'MCP fixture task'});assert.equal(change.status,200);assert.equal(change.body.title,'MCP fixture task');assert.equal(await count(),before+1);
}finally{scoped.close();}
const general=await connect(['mcp','--allow-local']);
try{
 const tools=(await general.rpc('tools/list',{})).tools;
 for(const name of ['hycli_prepare','hycli_sites','hycli_run','hycli_accounts','hycli_writes'])assert.ok(tools.some(t=>t.name===name),name);
 assert.equal(general.initialized.capabilities.tools.listChanged,true);
 const read=await general.call('hycli_run',{site:'library',action:'find-articles',inputs:{q:'fixture'}});assert.equal(read.status,200);
 // Install after the MCP handshake: the same connection must discover and run new tools.
 const specFile=path.join(ports.data,'mcp-new-site.json');
 await fs.writeFile(specFile,JSON.stringify({site:{name:'mcp-new-site',title:'New sample collection',base_url:ports.website},operations:[{name:'list-articles',method:'GET',path:'/articles',effect:'read',evidence:'Synthetic fixture documentation'}]}));
 await new Promise((resolve,reject)=>{const install=spawn(binary,['spec','install',specFile],{env:{...process.env,HYCLI_DATA_DIR:ports.data},stdio:['ignore','ignore','pipe']});let output='';install.stderr.on('data',chunk=>output+=chunk);install.on('error',reject);install.on('exit',code=>code===0?resolve():reject(Error(output)));});
 const fresh=await general.call('hycli_run',{site:'mcp-new-site',action:'list-articles'});assert.equal(fresh.status,200);
 for(let i=0;i<40&&!general.notifications.some(n=>n.method==='notifications/tools/list_changed');i++)await sleep(250);
 assert.ok(general.notifications.some(n=>n.method==='notifications/tools/list_changed'));
 assert.ok((await general.rpc('tools/list',{})).tools.some(t=>t.name==='mcp-new-site_list-articles'));
 await general.call('hycli_writes',{site:'library',allow:false});
 const refusedRun=await general.rpc('tools/call',{name:'hycli_run',arguments:{site:'library',action:'create-task',inputs:{title:'Blocked'}}});assert.ok(refusedRun.isError);assert.match(refusedRun.content[0].text,/writes_disabled/);
 await general.call('hycli_writes',{site:'library',allow:true});
 const before=await count();const created=await general.call('hycli_run',{site:'library',action:'create-task',inputs:{title:'Generic MCP action'}});assert.equal(created.status,200);assert.equal(created.body.title,'Generic MCP action');assert.equal(await count(),before+1);
 assert.equal((await fetch(ports.dashboard+'/api/sites/mcp-new-site',{method:'DELETE',headers:auth})).status,200);
 assert.equal((await allowChanges(false)).status,200);
 await fs.rm(specFile);
 console.log(JSON.stringify({mcp:'passed',scoped:'one website',general:'preparation and generic execution',installed_after_connect:'usable with tool-list notification',read:'sanitized',write:'per-CLI write permission and direct execution'},null,2));
}finally{general.close();}

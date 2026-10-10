#!/usr/bin/env node
// Exercise an extracted release with a fresh local data directory and no development tools on PATH.
import fs from 'node:fs/promises';
import path from 'node:path';
import net from 'node:net';
import {spawn,spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
const root=path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const archive=process.argv[2];if(!archive)throw Error('Usage: node scripts/smoke-package.mjs <native.tar.gz>');
const scratch=path.join(root,'.cache','package-smoke');await fs.mkdir(scratch,{recursive:true});
const directory=await fs.mkdtemp(path.join(scratch,'fresh-'));
// Read from stdin so GNU tar cannot interpret a Windows drive letter as a remote host.
const extracted=spawnSync('tar',['-xzf','-'],{cwd:directory,input:await fs.readFile(path.resolve(archive)),stdio:['pipe','inherit','inherit']});
if(extracted.error)throw extracted.error;if(extracted.status!==0)throw Error('Cannot extract archive');
const entries=await fs.readdir(directory);if(entries.length!==1)throw Error('Expected a single package root');
const packageRoot=path.join(directory,entries[0]);
for(const line of (await fs.readFile(path.join(packageRoot,'SHA256SUMS'),'utf8')).trim().split('\n')){
 const match=line.match(/^([a-f0-9]{64})  (.+)$/);if(!match)throw Error('Invalid checksum list');
 if(createHash('sha256').update(await fs.readFile(path.join(packageRoot,match[2]))).digest('hex')!==match[1])throw Error(`Checksum mismatch: ${match[2]}`);
}
const executable=path.join(packageRoot,process.platform==='win32'?'hycli.exe':'hycli');
const data=path.join(directory,'fresh-data'),cwd=path.join(directory,'outside-checkout');await fs.mkdir(cwd);
const env={...process.env,PATH:packageRoot,HYCLI_DATA_DIR:data};
delete env.HYCLI_SPECS;delete env.HYCLI_KB;
const startupTimeout=Number(process.env.HYCLI_SMOKE_TIMEOUT_MS)||90000;
for(const args of [['--version'],['describe'],['accounts']]){
 const result=spawnSync(executable,args,{cwd,env,encoding:'utf8'});
 if(result.error)throw result.error;if(result.status!==0)throw Error(`${args.join(' ')}: ${result.stderr}`);
 if(args[0]!=='--version'){const value=JSON.parse(result.stdout),items=args[0]==='describe'?value.sites:value;if(!Array.isArray(items)||items.length)throw Error('Fresh package contains user data');}
}
const listener=net.createServer();await new Promise((resolve,reject)=>{listener.once('error',reject);listener.listen(0,'127.0.0.1',resolve);});const port=listener.address().port;await new Promise(resolve=>listener.close(resolve));
const dashboard=spawn(executable,['dashboard','--no-open','--port',String(port)],{cwd,env,stdio:['ignore','pipe','pipe']});
let startup='',launchError;dashboard.stderr.on('data',chunk=>startup+=chunk);dashboard.on('error',error=>launchError=error);
const base=`http://127.0.0.1:${port}`,sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
let mcp;
try{
 let response;const deadline=Date.now()+startupTimeout;
 while(Date.now()<deadline){if(launchError)throw launchError;if(dashboard.exitCode!==null)throw Error(startup);try{response=await fetch(base,{signal:AbortSignal.timeout(2000)});if(response.ok)break;}catch{}await sleep(200);}
 if(!response?.ok)throw Error('Dashboard startup timeout');
 const html=await response.text();const assets=[...html.matchAll(/(?:src|href)="([^"]+\.(?:js|css))"/g)].map(match=>match[1]);
 if(!assets.length)throw Error('Embedded dashboard assets missing');
 for(const document of ['/guide.html','/license.txt'])if(!(await fetch(base+document)).ok)throw Error('Missing offline documentation: '+document);
 for(const asset of assets)if(!(await fetch(new URL(asset,base))).ok)throw Error(`Embedded asset unavailable: ${asset}`);
 if((await fetch(base+'/api/session')).status!==401)throw Error('A plain local request minted a dashboard session');
 const launch=startup.match(/[?&]launch=([A-Za-z0-9]+)/)?.[1];if(!launch)throw Error('Missing launch address: '+startup);
 const session=await fetch(base+'/api/session?launch='+launch),cookie=session.headers.get('set-cookie')?.split(';')[0];
 if(!cookie)throw Error('Missing local session');
 const state=await (await fetch(base+'/api/state',{headers:{cookie}})).json();
 if(state.sites.length||state.accounts.length)throw Error('Release includes website/account data');
 mcp=spawn(executable,['mcp'],{cwd,env,stdio:['pipe','pipe','pipe']});
 let buffer='',sequence=0;const pending=new Map();
 const fail=error=>{for(const item of pending.values()){clearTimeout(item.timer);item.reject(error);}pending.clear();};mcp.on('error',fail);mcp.on('exit',code=>fail(Error(`MCP exited ${code}`)));
 mcp.stdout.on('data',chunk=>{buffer+=chunk;while(buffer.includes('\n')){const end=buffer.indexOf('\n'),line=buffer.slice(0,end);buffer=buffer.slice(end+1);if(!line.trim())continue;try{const message=JSON.parse(line),item=pending.get(message.id);if(item){clearTimeout(item.timer);pending.delete(message.id);message.error?item.reject(Error(message.error.message)):item.resolve(message.result);}}catch(error){fail(error);}}});
 const rpc=(method,params)=>new Promise((resolve,reject)=>{const id=++sequence,timer=setTimeout(()=>{pending.delete(id);reject(Error('MCP timeout: '+method));},30000);pending.set(id,{resolve,reject,timer});mcp.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n');});
 await rpc('initialize',{protocolVersion:'2025-03-26',capabilities:{},clientInfo:{name:'release-smoke',version:'1'}});mcp.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');
 const tools=(await rpc('tools/list',{})).tools;
 for(const name of ['hycli_prepare','hycli_run','hycli_sites','hycli_accounts'])if(!tools.some(tool=>tool.name===name))throw Error('Missing tool: '+name);
 const result=await rpc('tools/call',{name:'hycli_sites',arguments:{}});
 if(result.isError||JSON.parse(result.content.find(content=>content.type==='text').text).length!==0)throw Error('Fresh MCP websites are not empty');
 console.log(JSON.stringify({package:path.basename(archive),checksums:'passed',fresh_data:'empty',embedded_assets:assets.length,cli:'passed',mcp:'passed',runtime_path:'package directory only',directory}));
}finally{
 if(mcp){mcp.stdin.end();mcp.kill('SIGTERM');}
 dashboard.kill('SIGTERM');
}

#!/usr/bin/env node
import {spawn} from 'node:child_process';
import fs from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {parseArgs} from 'node:util';
const root=path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const {values}=parseArgs({options:{help:{type:'boolean'},full:{type:'boolean'}}});
if(values.help){console.log('Usage: node scripts/test-e2e.mjs [--full]\nRun actual Rust dashboard/MCP fixtures. Build the dashboard and cargo examples first.\n--full adds provider, 20-language layout and isolated browser-companion tests.\nEnvironment: HYCLI_BIN, HYCLI_FIXTURE_BIN, CARGO_TARGET_DIR, HYCLI_TEST_DATA, BROWSER_BIN, TEST_CDP, HYCLI_FIXTURE_START_TIMEOUT_MS.');process.exit(0);}
const target=path.resolve(root,process.env.CARGO_TARGET_DIR||'target');
const suffix=process.platform==='win32'?'.exe':'';
const nativeTarget=process.env.CARGO_BUILD_TARGET ? path.join(target,process.env.CARGO_BUILD_TARGET) : target;
const fixture=process.env.HYCLI_FIXTURE_BIN||path.join(nativeTarget,'debug','examples','dashboard_fixture'+suffix);
const binary=process.env.HYCLI_BIN||path.join(nativeTarget,'debug','hycli'+suffix);
await Promise.all([fs.access(fixture),fs.access(binary)]);
const scratch=path.resolve(process.env.HYCLI_TEST_DATA||path.join(root,'.cache'));
await fs.mkdir(scratch,{recursive:true});
const directory=await fs.mkdtemp(path.join(scratch,'e2e-'));
await fs.mkdir(path.join(directory,'tmp'),{recursive:true});
const output=path.join(directory,'ports.json');
const env={...process.env,HYCLI_BIN:binary,HYCLI_FIXTURE_OUTPUT:output,HYCLI_TEST_DATA:path.join(directory,'data'),TMPDIR:path.join(directory,'tmp'),TMP:path.join(directory,'tmp'),TEMP:path.join(directory,'tmp'),HYCLI_NO_SCREENSHOTS:process.env.HYCLI_NO_SCREENSHOTS||'1'};
const server=spawn(fixture,[],{cwd:root,env,stdio:['ignore','inherit','inherit']});
let exited=false;server.on('exit',()=>{exited=true;});let launchError;server.on('error',error=>{launchError=error;});
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
try{
 const until=Date.now()+(Number(process.env.HYCLI_FIXTURE_START_TIMEOUT_MS)||120000);let ready=false;
 while(Date.now()<until){if(launchError)throw launchError;if(exited)throw Error('Fixture exited before becoming ready');try{const ports=JSON.parse(await fs.readFile(output,'utf8'));ready=(await fetch(ports.dashboard,{signal:AbortSignal.timeout(5000)})).ok;}catch{}if(ready)break;await sleep(250);}
 if(!ready)throw Error('Fixture startup timed out');
 const suites=['e2e','mcp-e2e','lifecycle-e2e',...(values.full?['zai-e2e','layout-e2e','extension-e2e']:[])];
 for(const suite of suites){console.log(`\nRunning ${suite}`);await new Promise((resolve,reject)=>{const child=spawn(process.execPath,[path.join(root,'dashboard',suite+'.mjs')],{cwd:root,env,stdio:'inherit'});child.once('error',reject);child.once('exit',(code,signal)=>code===0?resolve():reject(Error(`${suite} failed: ${code??signal}`)));});}
 console.log(`\nPassed ${suites.length} real API/browser suites. Fixture data: ${directory}`);
}finally{if(!exited){server.kill('SIGTERM');await Promise.race([new Promise(resolve=>server.once('exit',resolve)),sleep(5000)]);if(!exited)server.kill('SIGKILL');}}

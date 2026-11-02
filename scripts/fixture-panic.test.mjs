import test from 'node:test';
import assert from 'node:assert/strict';
import {fixturePanicMonitor} from './fixture-panic.mjs';
test('fixture panic gate recognizes Rust banners across chunks and remains failed',()=>{
 for(const banner of ["Fixture ready\nthread 'tokio-runtime-worker' panicked at examples/dashboard_fixture.rs:39:1:\n", "Expected HTTP 503 failure\n\nthread 'tokio-runtime-worker' (123) panicked at examples/dashboard_fixture.rs:39:1:\n"]){
  for(let split=0;split<banner.length;split++){
   const monitor=fixturePanicMonitor();
   monitor.write(banner.slice(0,split));monitor.write(banner.slice(split));
   assert.throws(()=>monitor.check(),/Owned fixture panicked/);
   monitor.write('Expected application failure\n');
   assert.throws(()=>monitor.check(),/Owned fixture panicked/);
  }
 }
});
test('fixture gate does not treat expected application failures as panics',()=>{
 const monitor=fixturePanicMonitor();
 monitor.write('HTTP 503 Synthetic summary failure\njob failed: blocked_by_site\nassertion test description\n');
 assert.doesNotThrow(()=>monitor.check());
});

import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {spawn} from 'node:child_process';
test('runner fails on fixture teardown panic even when every suite passes',{skip:process.platform==='win32' && 'Synthetic executable fixture uses a POSIX shebang and SIGTERM handler'},async()=>{
 const root=await fs.mkdtemp(path.join(process.env.HYCLI_TEST_DATA||os.tmpdir(),'fixture-gate-'));
 try {
  await fs.mkdir(path.join(root,'scripts'));await fs.mkdir(path.join(root,'dashboard'));
  for(const name of ['test-e2e.mjs','fixture-panic.mjs']) await fs.copyFile(new URL(name,import.meta.url),path.join(root,'scripts',name));
  for(const name of ['e2e','mcp-e2e','lifecycle-e2e']) await fs.writeFile(path.join(root,'dashboard',name+'.mjs'),'process.exit(0);');
  const fixture=path.join(root,'fixture.mjs');
  await fs.writeFile(fixture,`#!/usr/bin/env node
import http from 'node:http';
import fs from 'node:fs';
const server=http.createServer((req,res)=>res.end('ready'));
server.listen(0,'127.0.0.1',()=>fs.writeFileSync(process.env.HYCLI_FIXTURE_OUTPUT,JSON.stringify({dashboard:'http://127.0.0.1:'+server.address().port})));
process.on('SIGTERM',()=>{process.stderr.write(process.env.TEST_FIXTURE_PANIC==='1'?"thread 'worker' (123) panicked at fixture.rs:1:1:\\n":'HTTP 503 Synthetic summary failure\\n',()=>server.close());});
`);
  await fs.chmod(fixture,0o700);
  for(const panic of ['0','1']) {
   const result=await new Promise((resolve,reject)=>{
    const child=spawn(process.execPath,[path.join(root,'scripts/test-e2e.mjs')],{env:{...process.env,HYCLI_BIN:process.execPath,HYCLI_FIXTURE_BIN:fixture,HYCLI_TEST_DATA:root,TEST_FIXTURE_PANIC:panic},stdio:['ignore','pipe','pipe']});
    let stdout='',stderr='';child.stdout.on('data',c=>stdout+=c);child.stderr.on('data',c=>stderr+=c);
    child.on('error',reject);child.on('close',code=>resolve({code,stdout,stderr}));
   });
   if(panic==='1') {
    assert.notEqual(result.code,0);assert.match(result.stderr,/Owned fixture panicked/);
    assert.match(result.stderr,/panicked at fixture.rs/);assert.doesNotMatch(result.stdout,/Passed 3/);
   } else {assert.equal(result.code,0,result.stderr);assert.match(result.stdout,/Passed 3/);}
  }
 } finally {await fs.rm(root,{recursive:true,force:true});}
});

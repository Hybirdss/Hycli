#!/usr/bin/env node
// Exercise real background processes with isolated data, including races and stale discovery.
import fs from 'node:fs/promises';
import path from 'node:path';
import net from 'node:net';
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';
const root=path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const executable=path.resolve(process.argv[2]||path.join(root,'dist',process.platform==='win32'?'hycli.exe':'hycli'));
const scratch=path.join(root,'.cache/desktop-smoke');await fs.mkdir(scratch,{recursive:true});
const directory=await fs.mkdtemp(path.join(scratch,'fresh-'));
const data=path.join(directory,'data with spaces'),cwd=path.join(directory,'outside checkout');await fs.mkdir(cwd);
const env={...process.env,HYCLI_DATA_DIR:data,HYCLI_KB:path.join(data,'kb.db'),HYCLI_SPECS:path.join(data,'specs')};
// Do not use a real browser or user profiles for lifecycle checks.
function run(args,options={}){return new Promise((resolve,reject)=>{
 const child=spawn(options.executable||executable,args,{cwd,env,stdio:['ignore','pipe','pipe'],...options});let stdout='',stderr='',settled=false;
 const finish=code=>{if(settled)return;settled=true;clearTimeout(timer);child.stdout.destroy();child.stderr.destroy();code===0?resolve(stdout):reject(Error(`${args.join(' ')} (${code}): ${stderr}${stdout}`));};
 child.stdout.on('data',chunk=>stdout+=chunk);child.stderr.on('data',chunk=>stderr+=chunk);child.on('error',reject);
 const timer=setTimeout(()=>{child.kill();reject(Error('Command timed out: '+args.join(' ')));},90000);
 child.once('close',finish);child.once('exit',code=>setTimeout(()=>finish(code),250));
});}
const json=async args=>JSON.parse(await run(args));
const blocker=net.createServer(socket=>socket.end('HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n',()=>socket.destroy()));
await new Promise((resolve,reject)=>{blocker.once('error',reject);blocker.listen(0,'127.0.0.1',resolve);});
const occupied=blocker.address().port;
const protectedTransport = {status: 'NOT RUN', reason: 'Private-file/native-browser bootstrap requires separate permitted browser verification; CLI lifecycle is not equivalent coverage.'};
console.warn('NOT RUN: native bootstrap transport, authenticated shutdown origin check, settings persistence, and browser Quit. This script verifies production CLI lifecycle only.');
try{
 assert.equal((await json(['status'])).running,false);
 // Concurrent opens converge; an occupied preferred port is never reused as if it were Hycli.
 const instances=await Promise.all(Array.from({length:3},()=>json(['open','--no-open','--port',String(occupied)])));
 const printed=instances[0].url;assert(instances.every(i=>i.url===printed));const publicUrl=new URL(printed);
 assert.equal(publicUrl.search,'');assert.equal(publicUrl.hash,'');assert.equal(publicUrl.username,'');assert.equal(publicUrl.password,'');
 const base=publicUrl.origin;assert.notEqual(publicUrl.port,String(occupied));
 // Only inspect public command output, never read a private opening file or ticket.
 assert(instances.every(i=>i.running===true&&typeof i.opening_file==='string'&&path.isAbsolute(i.opening_file)));
 assert.deepEqual((await json(['describe'])).sites,[]);assert.deepEqual(await json(['accounts']),[]);
 assert.equal((await json(['status'])).url,printed);
 assert.equal((await fetch(base+'/api/session')).status,401);
 assert.equal((await fetch(base+'/api/state')).status,401);
 assert.equal((await fetch(base+'/api/shutdown',{method:'POST'})).status,401);
 await run(['stop']);assert.equal((await json(['status'])).running,false);
 await run(['stop']); // idempotent
 // A stale record pointing at some other listener must never authorize stopping that listener.
 await fs.writeFile(path.join(data,'dashboard-instance.json'),JSON.stringify({port:occupied,id:'a'.repeat(32)}));
 assert.equal((await json(['status'])).running,false);await run(['stop']);assert.equal(blocker.listening,true);
 const launcher=path.join(path.dirname(executable),process.platform==='win32'?'hycli-desktop.exe':'hycli-desktop');
 await run(['open','--no-open','--port','0'],{executable:launcher});
 const restarted=await json(['status']);assert.equal(restarted.running,true);const restartedBase=new URL(restarted.url).origin;
 assert.equal(new URL(restarted.url).search,'');assert.equal(new URL(restarted.url).hash,'');
 assert.deepEqual((await json(['describe'])).sites,[]);assert.deepEqual(await json(['accounts']),[]);
 assert.equal((await fetch(restartedBase+'/api/session')).status,401);
 // --browser is retained as an explicit verification request, not silently reported as a pass.
 // Do not substitute direct private-file reads or an HTTP fragment for the native handoff.
 await run(['stop']);
 for(let i=0;i<100&&(await json(['status'])).running;i++)await new Promise(resolve=>setTimeout(resolve,100));
 assert.equal((await json(['status'])).running,false);
 console.log(JSON.stringify({executable,lifecycle:'passed',concurrent_open:3,port_conflict:'passed',stale_instance:'passed',unauthenticated_shutdown_guard:'passed',fresh_cli_data:'empty before and after restart',launcher:'passed',protected_transport:protectedTransport,cross_origin_authenticated_shutdown:'NOT RUN',saved_settings_restart:'NOT RUN',browser_quit:process.argv.includes('--browser')?'NOT RUN: protected native handoff unavailable':'NOT RUN: not requested',directory}));
 if(process.argv.includes('--browser'))process.exitCode=1; // Requested browser coverage remains unverified.
}finally{
 await run(['stop']).catch(()=>{});await new Promise(resolve=>blocker.close(resolve));
}

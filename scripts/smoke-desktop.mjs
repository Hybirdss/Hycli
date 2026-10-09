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
 const child=spawn(options.executable||executable,args,{cwd,env,stdio:['ignore','pipe','pipe'],...options});let stdout='',stderr='';
 child.stdout.on('data',chunk=>stdout+=chunk);child.stderr.on('data',chunk=>stderr+=chunk);child.on('error',reject);
 const timer=setTimeout(()=>{child.kill();reject(Error('Command timed out: '+args.join(' ')));},90000);
 child.once('close',code=>{clearTimeout(timer);code===0?resolve(stdout):reject(Error(`${args.join(' ')} (${code}): ${stderr}${stdout}`));});
});}
const json=async args=>JSON.parse(await run(args));
const blocker=net.createServer(socket=>socket.end('HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n',()=>socket.destroy()));
await new Promise((resolve,reject)=>{blocker.once('error',reject);blocker.listen(0,'127.0.0.1',resolve);});
const occupied=blocker.address().port;
let browser;
try{
 assert.equal((await json(['status'])).running,false);
 // Concurrent opens converge; an occupied preferred port is never reused as if it were Hycli.
 const instances=await Promise.all(Array.from({length:3},()=>json(['open','--no-open','--port',String(occupied)])));
 const base=instances[0].url;assert(instances.every(i=>i.url===base));assert.notEqual(new URL(base).port,String(occupied));
 assert.equal((await json(['status'])).url,base);
 const descriptor=JSON.parse(await fs.readFile(path.join(data,'dashboard-instance.json'),'utf8'));
 const response=await fetch(base+'/api/session'),cookie=response.headers.get('set-cookie').split(';')[0];const session=await response.json();
 assert.equal(session.instance_id,descriptor.id);
 assert.equal((await fetch(base+'/api/shutdown',{method:'POST'})).status,401);
 assert.equal((await fetch(base+'/api/shutdown',{method:'POST',headers:{cookie,'x-hycli-csrf':session.csrf,origin:'https://example.invalid'}})).status,403);
 const saved=await fetch(base+'/api/settings',{method:'PATCH',headers:{cookie,'x-hycli-csrf':session.csrf,'content-type':'application/json'},body:JSON.stringify({locale:'ko'})});assert.equal(saved.status,200);
 await run(['stop']);assert.equal((await json(['status'])).running,false);
 await run(['stop']); // idempotent
 // A stale record pointing at some other listener must never authorize stopping that listener.
 await fs.writeFile(path.join(data,'dashboard-instance.json'),JSON.stringify({port:occupied,id:'a'.repeat(32)}));
 assert.equal((await json(['status'])).running,false);await run(['stop']);assert.equal(blocker.listening,true);
 const launcher=path.join(path.dirname(executable),process.platform==='win32'?'hycli-desktop.exe':'hycli-desktop');
 await run(['open','--no-open','--port','0'],{executable:launcher});
 const restarted=await json(['status']);assert.equal(restarted.running,true);
 const renewed=await fetch(restarted.url+'/api/session'),renewedCookie=renewed.headers.get('set-cookie').split(';')[0];
 const state=await (await fetch(restarted.url+'/api/state',{headers:{cookie:renewedCookie}})).json();
 assert.equal(state.settings.locale,'ko');assert.equal(state.sites.length,0);assert.equal(state.accounts.length,0);
 if(process.argv.includes('--browser')){
  const {chromium}=await import('../dashboard/node_modules/playwright-core/index.mjs');
  browser=await chromium.launch({headless:true,...(process.env.BROWSER_BIN?{executablePath:process.env.BROWSER_BIN}:{})});
  const page=await browser.newPage();await page.goto(restarted.url+'/#settings');
  await page.getByRole('button',{name:'Hycli 종료',exact:true}).click();
  await page.getByRole('button',{name:'확인',exact:true}).click();
  await page.getByRole('heading',{name:'Hycli가 종료되었습니다',exact:true}).waitFor();
  await page.screenshot({path:path.join(directory,'quit.png')});
  await browser.close();browser=null;
 }else await run(['stop']);
 for(let i=0;i<100&&(await json(['status'])).running;i++)await new Promise(resolve=>setTimeout(resolve,100));
 assert.equal((await json(['status'])).running,false);
 console.log(JSON.stringify({executable,lifecycle:'passed',concurrent_open:3,port_conflict:'passed',stale_instance:'passed',shutdown_guard:'passed',saved_data:'preserved',launcher:'passed',browser_quit:process.argv.includes('--browser')?'passed':'not requested',directory}));
}finally{
 if(browser)await browser.close();await run(['stop']).catch(()=>{});await new Promise(resolve=>blocker.close(resolve));
}

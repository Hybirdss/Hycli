import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import {stripTypeScriptTypes} from 'node:module';
import {test} from 'node:test';
const source=fs.readFileSync(new URL('./src/api.ts',import.meta.url),'utf8').replaceAll('export ','');
function api(fetch){const context={fetch,URL,URLSearchParams,Response,location:{search:''}};vm.createContext(context);vm.runInContext(stripTypeScriptTypes(source+'\nglobalThis.request=request;',{mode:'transform'}),context);return context;}
test('native request preserves cancellation without converting it into a connection error',async()=>{
 const controller=new AbortController();const cancelled=new DOMException('Cancelled','AbortError');let started;
 const context=api(async(path,{signal})=>{assert.equal(signal,controller.signal);started=true;return new Promise((resolve,reject)=>signal.addEventListener('abort',()=>reject(cancelled),{once:true}));});
 const pending=context.request('/api/browser/find','POST',{},controller.signal);assert.ok(started);controller.abort();await assert.rejects(pending,e=>e===cancelled);
});
test('cancelling during session renewal never replays native discovery',async()=>{
 const controller=new AbortController();let calls=0,renew,started;
 const context=api(async path=>{if(path==='/api/session'){started=true;return new Promise(resolve=>renew=()=>resolve(new Response(JSON.stringify({csrf:'synthetic'}))));}calls++;return new Response(JSON.stringify({error:{code:'session_expired'}}),{status:401});});
 const pending=context.request('/api/browser/find','POST',{},controller.signal);
 while(!started)await new Promise(resolve=>setImmediate(resolve));controller.abort();renew();await assert.rejects(pending,e=>e.name==='AbortError');assert.equal(calls,1);
});
test('already cancelled native request never sends a request',async()=>{let calls=0;const context=api(async()=>{calls++;});const controller=new AbortController();controller.abort();await assert.rejects(context.request('/api/browser/find','POST',{},controller.signal),e=>e.name==='AbortError');assert.equal(calls,0);});

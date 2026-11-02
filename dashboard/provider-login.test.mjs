import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import vm from 'node:vm';
const source = readFileSync(new URL('./src/dialogs.tsx', import.meta.url), 'utf8');
const start = source.indexOf('let codexLoginStart:');
const end = source.indexOf('  return <Modal', start);
const code = stripTypeScriptTypes(source.slice(start, end).replace('export ', '') + '\nreturn { signIn, chooseProvider, checkAccount, retryLogin, closeDialog, submit, view: { login, account, error, busy, model, defaultProvider, verified } }; }');
const deferred = () => { let resolve, reject; const promise = new Promise((a,b) => {resolve=a;reject=b;}); return {promise,resolve,reject}; };
const tick = () => new Promise(resolve => setImmediate(resolve));
function harness(respond = async () => ({}), refresh = async () => {}) {
  const calls=[], toasts=[], writes=[], cells=[], effects=[], cleanups=[];
  let index=0, mounted=true, scheduled=false, handlers, time=0, timerId=0;
  const timers=new Map();
  const timer=(fn,ms,interval=false)=>{const id=++timerId;timers.set(id,{fn,ms,interval,at:time+ms});return id;};
  const context={AbortController, URL, ApiError: class extends Error {constructor(code,status){super(code);this.code=code;this.status=status;}}, effects, useI18n:()=>({t:x=>x}), codeOf:e=>e.code||'internal',
    request:async(...args)=>{calls.push(args);return respond(...args);},
    window:{setInterval:(fn,ms)=>timer(fn,ms,true),setTimeout:(fn,ms)=>timer(fn,ms)},
    setTimeout:(fn,ms)=>timer(fn,ms), clearTimeout:id=>timers.delete(id), clearInterval:id=>timers.delete(id),
    useState:initial=>{const i=index++;if(!(i in cells))cells[i]=typeof initial==='function'?initial():initial;return [cells[i],value=>{writes.push(value);cells[i]=typeof value==='function'?value(cells[i]):value;if(mounted&&!scheduled){scheduled=true;queueMicrotask(()=>{scheduled=false;if(mounted)render();});}}];},
    useRef:initial=>{const i=index++;return cells[i]??=({current:initial});},
    useEffect:(fn,deps)=>{const i=index++;const old=cells[i];if(!old||!deps||deps.some((d,j)=>d!==old[j])){cells[i]=deps;effects.push(()=>{cleanups[i]?.();cleanups[i]=fn();});}},
  };
  vm.createContext(context);vm.runInContext(code,context);
  const data={providers:[{id:'codex',model:'chosen',default_model:'default',connected:false,available:true,models:[],verification:'unknown'},{id:'openai',model:'api',default_model:'api-default',connected:false,models:[]}],settings:{default_provider:''}};
  const ops={refresh,act:async(...args)=>{calls.push(args);return respond(...args);},toast:message=>toasts.push(message)};
  function render(){index=0;handlers=context.ProviderDialog({provider:data.providers[0],data,ops,onClose(){}});while(effects.length)effects.shift()();}
  render();
  return {calls,toasts,writes,get actions(){return handlers;},async flush(){await tick();},unmount(){mounted=false;cleanups.forEach(fn=>fn?.());},async advance(ms){const end=time+ms;while(true){const next=[...timers].filter(([,t])=>t.at<=end).sort((a,b)=>a[1].at-b[1].at)[0];if(!next)break;const[id,t]=next;time=t.at;if(t.interval)t.at+=t.ms;else timers.delete(id);t.fn();await tick();}time=end;},values:cells};
}
test('closing during login start cancels only its returned login and publishes no stale state',async()=>{
 const start=deferred();const h=harness(path=>path.endsWith('/login')?start.promise:{});const pending=h.actions.signIn();await h.flush();h.unmount();const before=h.writes.length;start.resolve({login_id:'A',auth_url:'https://auth.openai.com/A'});await pending;await h.flush();assert.equal(h.writes.length,before);assert.equal(h.calls.filter(c=>c[0].endsWith('/cancel')&&c[2]?.login_id==='A').length,1);assert.deepEqual(h.toasts,[]);
});
test('slow login status requests never overlap',async()=>{
 const status=deferred();const h=harness(path=>path.endsWith('/login')?{login_id:'A',auth_url:'https://auth.openai.com/A'}:path.includes('/account')?status.promise:{});await h.actions.signIn();await h.flush();await h.advance(10000);assert.equal(h.calls.filter(c=>c[0].includes('/account')&&!c[3]?.aborted).length,1);h.unmount();status.resolve({connected:false,login_status:'pending'});await h.flush();
});
test('opening the dialog probes passively and does not ask to connect',async()=>{
 const h=harness(async()=>({connected:true,verification:'verified',account:'Synthetic'}));await h.flush();assert.equal(h.calls.length,1);assert.equal(h.calls[0][0],'/api/providers/codex/account');assert.equal(h.calls[0][1],'GET');assert.deepEqual(h.toasts,[]);assert.equal(h.actions.view.verified,true);h.unmount();
});
test('poll failure is visible, pauses automatically, and retry reuses the owned login',async()=>{
 let failed=true;const h=harness(async path=>path.endsWith('/login')?{login_id:'A',auth_url:'https://auth.openai.com/A'}:path.includes('login_id=')?(failed?Promise.reject({code:'timeout'}):{connected:true,verification:'verified',login_status:'completed'}):{connected:false,verification:'signed_out'});
 await h.flush();await h.actions.signIn();await h.flush();await h.advance(2500);assert.equal(h.actions.view.error,'timeout');const count=h.calls.length;await h.advance(20000);assert.equal(h.calls.length,count);failed=false;h.actions.retryLogin();await h.flush();assert.equal(h.actions.view.error,'');assert.equal(h.actions.view.login,null);assert.deepEqual(h.toasts,['connections.codexReady']);assert.equal(h.calls.filter(c=>c[0].endsWith('/login')).length,1);h.unmount();
});
test('closing a pending poll aborts the request and ignores even an abort-insensitive response',async()=>{
 const poll=deferred();const h=harness(path=>path.endsWith('/login')?{login_id:'A',auth_url:'https://auth.openai.com/A'}:path.includes('login_id=')?poll.promise:{connected:false,verification:'signed_out'});await h.flush();await h.actions.signIn();await h.flush();await h.advance(2500);const signal=h.calls.find(c=>c[0].includes('login_id='))[3];h.unmount();const before=h.writes.length;assert.equal(signal.aborted,true);poll.resolve({connected:true,verification:'verified',login_status:'completed'});await h.flush();assert.equal(h.writes.length,before);assert.deepEqual(h.toasts,[]);await h.advance(10000);assert.equal(h.calls.filter(c=>c[0].includes('login_id=')).length,1);
});
test('switching API/login methods clears the pending URL and cancels only that attempt',async()=>{
 const h=harness(path=>path.endsWith('/login')?{login_id:'A',auth_url:'https://auth.openai.com/A'}:{connected:false,verification:'signed_out'});await h.flush();await h.actions.signIn();await h.flush();assert.equal(h.actions.view.login.id,'A');h.actions.chooseProvider('openai');await h.flush();assert.equal(h.actions.view.login,null);assert.equal(h.actions.view.model,'api');assert.equal(h.calls.filter(c=>c[0].endsWith('/cancel')&&c[2]?.login_id==='A').length,1);await h.advance(10000);assert.equal(h.calls.filter(c=>c[0].includes('login_id=')).length,0);h.unmount();
});
test('a newer start waits for stale start receipt, then stale cleanup cannot cancel it',async()=>{
 const A=deferred();let starts=0;const h=harness(path=>path.endsWith('/login')?(++starts===1?A.promise:{login_id:'B',auth_url:'https://auth.openai.com/B'}):{connected:false,verification:'signed_out'});await h.flush();const pending=h.actions.signIn();await h.flush();h.actions.chooseProvider('openai');await h.flush();h.actions.chooseProvider('codex');await h.flush();const replacement=h.actions.signIn();await h.flush();assert.equal(starts,1);A.resolve({login_id:'A',auth_url:'https://auth.openai.com/A'});await pending;await replacement;await h.flush();assert.equal(starts,2);assert.equal(h.actions.view.login.id,'B');assert.deepEqual(h.calls.filter(c=>c[0].endsWith('/cancel')).map(c=>c[2].login_id),['A']);h.unmount();
});
test('existing account is not proof a pending login attempt completed',async()=>{
 let completed=false;const h=harness(path=>path.endsWith('/login')?{login_id:'A',auth_url:'https://auth.openai.com/A'}:{connected:true,verification:'verified',login_status:completed?'completed':'pending'});await h.flush();await h.actions.signIn();await h.flush();await h.advance(2500);assert.equal(h.actions.view.login.id,'A');assert.deepEqual(h.toasts,[]);completed=true;await h.advance(2500);assert.equal(h.actions.view.login,null);assert.deepEqual(h.toasts,['connections.codexReady']);h.unmount();
});
test('switching method while completion refresh is pending suppresses old success feedback',async()=>{
 const refresh=deferred();let delay=false;const h=harness(path=>path.endsWith('/login')?{login_id:'A',auth_url:'https://auth.openai.com/A'}:{connected:true,verification:'verified',login_status:'completed'},()=>delay?refresh.promise:Promise.resolve());await h.flush();delay=true;await h.actions.signIn();await h.flush();await h.advance(2500);h.actions.chooseProvider('openai');await h.flush();refresh.resolve();await h.flush();assert.deepEqual(h.toasts,[]);assert.equal(h.actions.view.login,null);h.unmount();
});
test('model discovery error does not hide a verified account or demand another login',async()=>{
 const h=harness(async()=>({connected:true,verification:'verified',account:'Synthetic',models_error:'ai_unavailable'}));await h.flush();await h.actions.checkAccount();await h.flush();assert.equal(h.actions.view.verified,true);assert.equal(h.actions.view.error,'ai_unavailable');assert.equal(h.actions.view.model,'chosen');assert.equal(h.calls.filter(c=>c[0].endsWith('/login')).length,0);h.unmount();
});
test('closing during passive probe suppresses its error and leaves no poll',async()=>{
 const check=deferred();const h=harness(()=>check.promise);h.unmount();const before=h.writes.length;check.reject({code:'timeout'});await h.flush();assert.equal(h.writes.length,before);await h.advance(10000);assert.equal(h.calls.length,1);
});
test('a failed current probe replaces old live success with unverified state and retry recovers',async()=>{
 let fail=false;const h=harness(async()=>fail?Promise.reject({code:'timeout'}):{connected:true,verification:'verified',account:'Synthetic'});await h.flush();assert.equal(h.actions.view.verified,true);fail=true;await h.actions.checkAccount();await h.flush();assert.equal(h.actions.view.verified,false);assert.equal(h.actions.view.account.verification,'check_failed');assert.equal(h.actions.view.model,'chosen');fail=false;await h.actions.checkAccount();await h.flush();assert.equal(h.actions.view.verified,true);h.unmount();
});
test('snapshot refresh failure is not a verified account logout',async()=>{
 let fail=false;const h=harness(async()=>({connected:true,verification:'verified',account:'Synthetic'}),async()=>{if(fail)throw {code:'connection_lost'};});await h.flush();fail=true;await h.actions.checkAccount();await h.flush();assert.equal(h.actions.view.verified,true);assert.equal(h.actions.view.error,'connection_lost');h.unmount();
});
test('official signed-out response is not displayed as a transient check failure',async()=>{
 const h=harness(async(path,method)=>method==='POST'?Promise.reject({code:'auth_required'}):{connected:false,verification:'signed_out'});await h.flush();await h.actions.checkAccount();await h.flush();assert.equal(h.actions.view.account.verification,'signed_out');assert.equal(h.actions.view.verified,false);assert.equal(h.actions.view.error,'auth_required');h.unmount();
});
test('an old A poll finishing after B completes cannot restore A or emit another success',async()=>{
 const A=deferred();let starts=0;const h=harness(path=>path.endsWith('/login')?{login_id:++starts===1?'A':'B',auth_url:'https://auth.openai.com/synthetic'}:path.includes('login_id=A')?A.promise:path.includes('login_id=B')?{connected:true,verification:'verified',login_status:'completed'}:{connected:false,verification:'signed_out'});await h.flush();await h.actions.signIn();await h.flush();await h.advance(2500);await h.actions.signIn();await h.flush();await h.advance(2500);assert.deepEqual(h.toasts,['connections.codexReady']);assert.equal(h.actions.view.login,null);A.resolve({connected:true,verification:'verified',login_status:'completed'});await h.flush();assert.deepEqual(h.toasts,['connections.codexReady']);assert.equal(h.actions.view.login,null);assert.deepEqual(h.calls.filter(c=>c[0].endsWith('/cancel')).map(c=>c[2].login_id),['A']);h.unmount();
});
test('a stalled start has a bounded timeout, releases B, and cleans up an unexpected late A receipt',async()=>{
 const A=deferred();let starts=0;const h=harness(path=>path.endsWith('/login')?(++starts===1?A.promise:{login_id:'B',auth_url:'https://auth.openai.com/B'}):{connected:false,verification:'signed_out'});await h.flush();const pending=h.actions.signIn();await h.flush();const replacement=h.actions.signIn();await h.flush();assert.equal(starts,1);await h.advance(50000);await pending;await replacement;await h.flush();assert.equal(h.actions.view.login.id,'B');assert.equal(h.calls.filter(c=>c[0].endsWith('/login'))[0][3].aborted,true);A.resolve({login_id:'A',auth_url:'https://auth.openai.com/A'});await h.flush();assert.equal(h.actions.view.login.id,'B');assert.deepEqual(h.calls.filter(c=>c[0].endsWith('/cancel')).map(c=>c[2].login_id),['A']);h.unmount();
});
test('a cancelled or failed owned attempt removes its stale continuation link',async()=>{
 for(const failure of ['cancelled','failed']){
 const h=harness(path=>path.endsWith('/login')?{login_id:'A',auth_url:'https://auth.openai.com/A'}:path.includes('login_id=')?(failure==='cancelled'?Promise.reject({code:'cancelled'}):{connected:false,verification:'signed_out',login_status:'failed'}):{connected:false,verification:'signed_out'});await h.flush();await h.actions.signIn();await h.flush();await h.advance(2500);assert.equal(h.actions.view.login,null);assert.equal(h.actions.view.error,'cancelled');const count=h.calls.length;await h.advance(10000);assert.equal(h.calls.length,count);h.unmount();
 }
});

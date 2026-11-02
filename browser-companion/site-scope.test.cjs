const assert = require('node:assert/strict');
const {test} = require('node:test');
const fs = require('node:fs');
const vm = require('node:vm');
function load() {
  const context = vm.createContext({URL,Date});
  for (const file of ['vendor/tldts.js','site-scope.js']) vm.runInContext(fs.readFileSync(__dirname+'/'+file,'utf8'),context);
  return context.HYCLI_SITE_SCOPE;
}
const scope = load();
const list = value => [...scope.plan(value).origins];
test('finite exact ancestors stop at ICANN and PRIVATE registrable boundaries',()=>{
  assert.deepEqual(list('https://a.b.example.co.uk/path'),['https://a.b.example.co.uk','https://b.example.co.uk','https://example.co.uk']);
  assert.deepEqual(list('https://www.alice.github.io'),['https://www.alice.github.io','https://alice.github.io']);
  assert.deepEqual(list('https://alice.github.io'),['https://alice.github.io']);
  assert.deepEqual(list('https://www.city.kawasaki.jp'),['https://www.city.kawasaki.jp','https://city.kawasaki.jp']);
  assert.deepEqual(list('https://a.b.ck'),['https://a.b.ck']);
});
test('IDNA, ports and unusual host forms cannot yield wildcard/public suffix grants',()=>{
  assert.deepEqual(list('https://www.bücher.de:8443'),['https://www.xn--bcher-kva.de:8443','https://xn--bcher-kva.de:8443']);
  for(const origin of ['https://co.uk','https://github.io','https://a.home.arpa','https://a.example.onion','http://localhost:1234','http://127.0.0.1:1234','http://[::1]:1234','https://a.unknownsuffix','https://www.example.com.']) assert.deepEqual(list(origin),[new URL(origin).origin]);
  assert.throws(()=>scope.plan('file:///tmp/test'));
  assert.throws(()=>scope.plan('https://user:pass@example.com'));
  assert.ok(!scope.plan('https://www.example.com').permissions.some(p=>p.includes('*.')));
});
const cookie = changes=>({name:'synthetic',value:'test',domain:'.example.com',hostOnly:false,path:'/',secure:true,storeId:'normal',...changes});
test('all imported cookies preserve exact hostOnly, store, path, secure and partition boundaries',()=>{
  const site=scope.plan('https://a.b.example.com');
  for(const c of [cookie(),cookie({domain:'.b.example.com',path:'/account'}),cookie({domain:'a.b.example.com',hostOnly:true}),cookie({partitionKey:{topLevelSite:'https://example.com',hasCrossSiteAncestor:false}})]) assert.equal(scope.allowsCookie(c,site,'normal'),true);
  for(const c of [cookie({domain:'example.com',hostOnly:true}),cookie({domain:'b.example.com',hostOnly:true}),cookie({domain:'.sibling.example.com'}),cookie({domain:'.com'}),cookie({storeId:'private'}),cookie({path:'bad'}),cookie({partitionKey:{topLevelSite:'https://other.com'}}),cookie({partitionKey:{topLevelSite:'http://example.com'}}),cookie({partitionKey:{topLevelSite:'https://example.com',hasCrossSiteAncestor:true}}),cookie({firstPartyDomain:'other.com'})]) assert.equal(scope.allowsCookie(c,site,'normal'),false,JSON.stringify(c));
  assert.equal(scope.allowsCookie(cookie(),scope.plan('http://www.example.com'),'normal'),false);
  assert.equal(scope.allowsCookie(cookie({domain:'.github.io'}),scope.plan('https://alice.github.io'),'normal'),false);
});
test('partition query append is filtered identically; API errors do not become logout',async()=>{
  const site=scope.plan('https://www.example.com');
  const bad=cookie({hostOnly:true,domain:'example.com'});
  const result=await scope.cookies({permissions:{contains:async()=>true},cookies:{getAll:async q=>q.partitionKey?[bad,cookie({storeId:'other'}),cookie({partitionKey:{topLevelSite:'https://other.com'}})]:[cookie(),bad]}},site,'normal',false);
  assert.equal(result.length,1);assert.equal(result[0].hostOnly,false);
  await assert.rejects(scope.cookies({permissions:{contains:async()=>true},cookies:{getAll:async()=>{throw Error('Unavailable');}}},site,'normal',false),/Unavailable/);
});
function background(overrides={}) {
  const calls=[];const handlers={};const site='https://www.example.com';
  const connection={siteOrigin:site,storeId:'normal',incognito:false,token:'synthetic',address:'http://127.0.0.1:4318',permissionOrigins:[site+'/*','https://example.com/*']};
  const events = name=>({addListener:fn=>{handlers[name]=fn;}});
  const context=vm.createContext({URL,Date,Map,TextDecoder,JSON,console,navigator:{userAgent:'Chrome'},setTimeout:fn=>{calls.push(['timer',fn]);return 1;},clearTimeout:()=>{},fetch:async(url,opts)=>{calls.push(['fetch',url,JSON.parse(opts.body)]);return{status:200};},chrome:{
    webRequest:{onBeforeRequest:events('before'),onCompleted:events('completed'),onErrorOccurred:events('error')},
    tabs:{get:async id=>({id,url:site,incognito:false})},
    cookies:{getAll:async()=>[cookie()],getAllCookieStores:async()=>[{id:'normal',tabIds:[1]}],onChanged:events('cookie')},
    permissions:{contains:async()=>true},
    storage:{local:{get:async()=>({['connection:'+site+':normal']:connection}),remove:async()=>{}},session:{get:async()=>({}),set:async value=>calls.push(['identity',value])}},runtime:{onStartup:events('startup')}
  }});
  for(const file of ['vendor/tldts.js','site-scope.js','background.js'])vm.runInContext(fs.readFileSync(__dirname+'/'+file,'utf8'),context);
  overrides.apply?.(context,connection);
  return {context,handlers,calls,connection,site};
}
test('cookie-only parent grants do not record or forward parent/sibling traffic',async()=>{
  for(const url of ['https://example.com/account','https://sibling.example.com/account','https://www.example.com:8443/account']){
    const b=background({apply:c=>{c.chrome.tabs.get=async id=>({id,url,incognito:false});}});
    await b.handlers.before({tabId:1,requestId:'r',method:'GET',url});
    await b.handlers.completed({tabId:1,requestId:'r',statusCode:200});
    assert.equal(b.calls.length,0);
  }
  const b=background();
  await b.handlers.before({tabId:1,requestId:'r',method:'GET',url:b.site+'/account'});
  await b.handlers.completed({tabId:1,requestId:'r',statusCode:200});
  assert.ok(b.calls.some(c=>c[0]==='identity'));
  const timer=b.calls.find(c=>c[0]==='timer');assert.ok(timer);await timer[1]();
  assert.equal(b.calls.filter(c=>c[0]==='fetch').length,1);
});
test('refresh denial, revocation, empty visibility and context changes preserve session',async()=>{
  for(const kind of ['denied','revoked','empty','changed','error']) {
    const b=background({apply:(c,connection)=>{
      if(kind==='denied') c.chrome.permissions.contains=async()=>false;
      if(kind==='revoked'){let n=0;c.chrome.permissions.contains=async()=>++n===1;}
      if(kind==='empty')c.chrome.cookies.getAll=async()=>[];
      if(kind==='changed')c.chrome.storage.local.get=async()=>({});
      if(kind==='error')c.chrome.cookies.getAll=async()=>{throw Error('unavailable');};
    }});
    await b.context.refreshConnection(b.site+':normal',b.connection);
    assert.equal(b.calls.filter(c=>c[0]==='fetch').length,0,kind);
  }
  const b=background();await b.context.refreshConnection(b.site+':normal',b.connection);
  const sends=b.calls.filter(c=>c[0]==='fetch');assert.equal(sends.length,1);
  assert.equal(sends[0][2].cookies.length,1);
});
test('revocation between cookie queries prevents the second read',async()=>{
  let reads=0,checks=0;
  await assert.rejects(scope.cookies({permissions:{contains:async()=>++checks===1},cookies:{getAll:async()=>{reads++;return[cookie()];}}},scope.plan('https://www.example.com'),'normal',false),/access is unavailable/);
  assert.equal(reads,1);
});
function popup(options={}) {
  const elements=new Map(); const calls=[];
  const element=id=>{if(!elements.has(id))elements.set(id,{value:id==='address'?'http://127.0.0.1:4318':'',dataset:{},add(){},setAttribute(){},addEventListener(event,fn){this[event]=fn;}});return elements.get(id);};
  const tab={id:1,url:'https://www.example.com/account',incognito:false};let queries=0;
  const chrome={i18n:{getUILanguage:()=> 'en'},storage:{local:{get:async()=>({}),set:async data=>calls.push(['save',data])},session:{get:async()=>({})}},
    permissions:{request:data=>{calls.push(['request',data]);return Promise.resolve(options.granted!==false);},contains:async()=>true},
    tabs:{query:async()=>[{...tab,...(options.changed && ++queries>1?{id:2}:{})}]},
    cookies:{getAllCookieStores:async()=>[{id:'normal',tabIds:[1]}],getAll:async()=>options.empty?[]:[cookie(),cookie({domain:'example.com',hostOnly:true})]}
  };
  const context=vm.createContext({URL,Date,Map,JSON,Option:function(){},document:{getElementById:element,documentElement:{},querySelectorAll:()=>[]},navigator:{userAgent:'Chrome'},chrome,
    fetch:async(url,opts)=>{calls.push(['fetch',url,JSON.parse(opts.body)]);return{ok:true,json:async()=>url.endsWith('/pair')?{url:tab.url,token:'synthetic'}:{session_token:'synthetic-next'}};}});
  for(const file of ['locales.js','vendor/tldts.js','site-scope.js','popup.js'])vm.runInContext(fs.readFileSync(__dirname+'/'+file,'utf8'),context);
  return{elements,element,calls,context};
}
test('popup displays finite scopes before synchronous user-gesture consent and transfers only applicable cookies',async()=>{
  const p=popup();await p.element('pair-form').submit({preventDefault(){}});
  assert.equal(p.element('permission-origins').textContent,'https://www.example.com/*\nhttps://example.com/*');
  const connecting=p.element('connect').click();
  const request=p.calls.find(c=>c[0]==='request');assert.ok(request,'request must occur before the first await');
  assert.deepEqual([...request[1].origins],['https://www.example.com/*','https://example.com/*']);
  await connecting;
  const connect=p.calls.find(c=>c[0]==='fetch' && c[1].endsWith('/connect'));assert.ok(connect);
  assert.equal(connect[2].cookies.length,1);assert.equal(connect[2].cookies[0].hostOnly,false);
  const saved=p.calls.find(c=>c[0]==='save')[1]['connection:https://www.example.com:normal'];
  assert.equal(saved.siteOrigin,'https://www.example.com');
});
test('popup denial, empty visibility and tab changes never connect or replace a session',async()=>{
  for(const options of [{granted:false},{empty:true},{changed:true}]){
    const p=popup(options);await p.element('pair-form').submit({preventDefault(){}});await p.element('connect').click();
    assert.equal(p.calls.filter(c=>c[0]==='fetch' && c[1].endsWith('/connect')).length,0);
    assert.equal(p.calls.filter(c=>c[0]==='save').length,0);
  }
});

test('Cookie host grants omit ports on both browsers without widening execution origin or partition site',()=>{
  const chrome=scope.plan('https://a.example.com:8443/path');
  const firefox=scope.plan('https://a.example.com:8443/path',true);
  assert.deepEqual([...chrome.permissions],['https://a.example.com/*','https://example.com/*']);
  assert.equal(chrome.origin,'https://a.example.com:8443');
  assert.deepEqual([...firefox.permissions],['https://a.example.com/*','https://example.com/*']);
  assert.equal(firefox.origin,'https://a.example.com:8443');
  assert.equal(firefox.topLevelSite,'https://example.com');
});

test('revoked or replaced connections cannot publish an already queued observation',async()=>{
  for(const mode of ['revoked','replaced']){
    const b=background();
    b.handlers.before({tabId:1,requestId:'r',method:'GET',url:b.site+'/items'});
    await b.handlers.completed({tabId:1,requestId:'r',statusCode:200});
    const timer=b.calls.find(c=>c[0]==='timer');assert.ok(timer);
    if(mode==='revoked')b.context.chrome.permissions.contains=async()=>false;
    else b.context.chrome.storage.local.get=async()=>({});
    await timer[1]();assert.equal(b.calls.filter(c=>c[0]==='fetch').length,0);
  }
});

test('delayed forbidden responses cannot delete a newly reconnected session',async()=>{
  for(const mode of ['refresh','observe']) {
    const b=background();let deleted=false;
    b.context.chrome.storage.local.remove=async()=>{deleted=true;};
    b.context.fetch=async()=>{
      b.context.chrome.storage.local.get=async()=>({['connection:'+b.site+':normal']:{...b.connection,token:'new-connection'}});
      return {status:403};
    };
    if(mode==='refresh') await b.context.refreshConnection(b.site+':normal',b.connection);
    else {
      b.handlers.before({tabId:1,requestId:'r',method:'GET',url:b.site+'/items'});
      await b.handlers.completed({tabId:1,requestId:'r',statusCode:200});
      await b.calls.find(c=>c[0]==='timer')[1]();
    }
    assert.equal(deleted,false,mode);
  }
});

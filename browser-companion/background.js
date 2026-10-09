// Observe only request shapes. Values, headers, cookies and response bodies are never recorded.
const pending = new Map();
const queues = new Map();
const timers = new Map();
function privateField(key) { return /token|password|passwd|secret|cookie|authorization|api.?key|csrf|xsrf|session.?id/i.test(key); }
function kind(value) { return typeof value==='boolean'?'bool':typeof value==='number'?(Number.isInteger(value)?'int':'float'):value&&typeof value==='object'?'json':'string'; }
chrome.webRequest.onBeforeRequest.addListener(details => {
  try {
    if (details.tabId<0 || pending.size>500 || !['GET','POST','PUT','PATCH','DELETE','HEAD'].includes(details.method)) return;
    const url=new URL(details.url); if(!['https:','http:'].includes(url.protocol))return;
    const query={}; for(const key of url.searchParams.keys())if(!privateField(key))query[key]='string';
    const body={}; const raw=details.requestBody;
    if(raw?.formData){for(const key of Object.keys(raw.formData))if(!privateField(key))body[key]='string';}
    else if(raw?.raw?.length===1&&raw.raw[0].bytes?.byteLength<128000){try{const parsed=JSON.parse(new TextDecoder().decode(raw.raw[0].bytes));if(parsed&&typeof parsed==='object'&&!Array.isArray(parsed))for(const [key,value] of Object.entries(parsed))if(!privateField(key))body[key]=kind(value);}catch{}}
    pending.set(details.requestId,{origin:url.origin,method:details.method,path:url.pathname,query,body,status:0});
  } catch {}
}, {urls:['https://*/*','http://*/*'],types:['xmlhttprequest']}, ['requestBody']);
chrome.webRequest.onErrorOccurred.addListener(details=>pending.delete(details.requestId),{urls:['https://*/*','http://*/*'],types:['xmlhttprequest']});
chrome.webRequest.onCompleted.addListener(async details => {
  const observation=pending.get(details.requestId);pending.delete(details.requestId);if(!observation)return;
  try {
    const tab=await chrome.tabs.get(details.tabId);if(!tab.url||new URL(tab.url).origin!==observation.origin)return;
    const scope=`${observation.origin}:${tab.incognito?'private':'normal'}`;
    if(observation.method==='GET'&&details.statusCode>=200&&details.statusCode<300&&!Object.keys(observation.query).length&&/(?:\/)(?:me|@me|viewer|userinfo|whoami|account|profile|session|current-user|current_user)\/?$/i.test(observation.path)){await chrome.storage.session.set({[`identity:${scope}`]:{url:observation.origin+observation.path,at:Date.now()}});}
    const connection=(await chrome.storage.local.get(`connection:${scope}`))[`connection:${scope}`];if(!connection)return;
    const {origin,...shape}=observation;shape.status=details.statusCode;
    const queue=queues.get(scope)||[];queue.push(shape);queues.set(scope,queue.slice(-40));
    if(timers.has(scope))return;
    timers.set(scope,setTimeout(async()=>{timers.delete(scope);const batch=queues.get(scope)||[];queues.delete(scope);try{const response=await fetch(connection.address+'/bridge/observe',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({token:connection.token,observations:batch}),credentials:'omit',redirect:'error'});if(response.status===403)await chrome.storage.local.remove(`connection:${scope}`);}catch{}},1500));
  } catch {}
},{urls:['https://*/*','http://*/*'],types:['xmlhttprequest']});

// Keep already-authorized website sessions current. Never request a new site permission here.
const refreshTimers = new Map();
async function refreshConnection(scope, connection) {
  if (!connection.storeId || !connection.siteOrigin) return;
  try {
    const site = new URL(connection.siteOrigin);
    if (!await chrome.permissions.contains({origins:[site.origin+'/*']})) return;
    const available = await chrome.cookies.getAll({storeId:connection.storeId,...(navigator.userAgent.includes('Firefox/')?{firstPartyDomain:null}:{})});
    const cookies = available.filter(cookie => { const domain=cookie.domain.replace(/^\./,''); return site.hostname===domain || (!cookie.hostOnly && site.hostname.endsWith('.'+domain)); });
    try { for (const cookie of await chrome.cookies.getAll({url:site.origin,storeId:connection.storeId,partitionKey:{topLevelSite:site.origin}})) if(!cookies.some(item=>item.name===cookie.name&&item.domain===cookie.domain&&item.path===cookie.path&&JSON.stringify(item.partitionKey)===JSON.stringify(cookie.partitionKey))) cookies.push(cookie); } catch {}
    const identity=(await chrome.storage.session.get('identity:'+scope))['identity:'+scope];
    const identity_source=identity && Date.now()-identity.at<30*60*1000?identity.url:'';
    const response=await fetch(connection.address+'/bridge/refresh',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({token:connection.token,cookies,identity_source}),credentials:'omit',redirect:'error'});
    if(response.status===403) await chrome.storage.local.remove('connection:'+scope);
  } catch {}
}
function scheduleRefresh(scope, connection) {
  clearTimeout(refreshTimers.get(scope));
  refreshTimers.set(scope,setTimeout(()=>{refreshTimers.delete(scope);void refreshConnection(scope,connection);},2000));
}
chrome.cookies.onChanged.addListener(async ({cookie})=>{
  const saved=await chrome.storage.local.get(null);
  for(const [key,connection] of Object.entries(saved)) if(key.startsWith('connection:') && connection.storeId===cookie.storeId && connection.siteOrigin) {
    const host=new URL(connection.siteOrigin).hostname;const domain=cookie.domain.replace(/^\./,'');
    if(host===domain||(!cookie.hostOnly&&host.endsWith('.'+domain))) scheduleRefresh(key.slice(11),connection);
  }
});
chrome.runtime.onStartup.addListener(async()=>{
  const saved=await chrome.storage.local.get(null);
  for(const [key,connection] of Object.entries(saved)) if(key.startsWith('connection:')) scheduleRefresh(key.slice(11),connection);
});

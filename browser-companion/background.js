if (typeof importScripts === 'function') importScripts('vendor/tldts.js', 'site-scope.js');
// Observe only request shapes. Values, headers, cookies and response bodies are never recorded.
const pending = new Map();
const queues = new Map();
const timers = new Map();
async function retireConnection(scope, connection) {
  const key='connection:'+scope;
  const current=(await chrome.storage.local.get(key))[key];
  if(current && current.token===connection.token && current.storeId===connection.storeId && current.siteOrigin===connection.siteOrigin && current.incognito===connection.incognito) await chrome.storage.local.remove(key);
}
function privateField(key) { return /token|password|passwd|secret|cookie|authorization|api.?key|csrf|xsrf|session.?id/i.test(key); }
function kind(value) { return typeof value==='boolean'?'bool':typeof value==='number'?(Number.isInteger(value)?'int':'float'):value&&typeof value==='object'?'json':'string'; }
async function captureRequest(details) {
  try {
    const url=new URL(details.url); if(!['https:','http:'].includes(url.protocol))return;
    // Parent-domain permissions only expose applicable cookies. They do not
    // authorize recording or forwarding traffic from parent/sibling websites.
    const tab=await chrome.tabs.get(details.tabId);
    if(!tab.url || new URL(tab.url).origin!==url.origin)return;
    const scope=`${url.origin}:${tab.incognito?'private':'normal'}`;
    const connection=(await chrome.storage.local.get(`connection:${scope}`))[`connection:${scope}`];
    if(!connection || connection.siteOrigin!==url.origin || connection.incognito!==!!tab.incognito)return;
    const stores=await chrome.cookies.getAllCookieStores();
    if(!stores.some(store=>store.id===connection.storeId && store.tabIds.includes(tab.id)))return;
    if(!await chrome.permissions.contains({origins:connection.permissionOrigins || [url.origin+'/*']}))return;
    const query={}; for(const key of url.searchParams.keys())if(!privateField(key))query[key]='string';
    const body={}; const raw=details.requestBody;
    if(raw?.formData){for(const key of Object.keys(raw.formData))if(!privateField(key))body[key]='string';}
    else if(raw?.raw?.length===1&&raw.raw[0].bytes?.byteLength<128000){try{const parsed=JSON.parse(new TextDecoder().decode(raw.raw[0].bytes));if(parsed&&typeof parsed==='object'&&!Array.isArray(parsed))for(const [key,value] of Object.entries(parsed))if(!privateField(key))body[key]=kind(value);}catch{}}
    return {connection,observation:{origin:url.origin,method:details.method,path:url.pathname,query,body,status:0}};
  } catch {}
}
chrome.webRequest.onBeforeRequest.addListener(details => {
  if(details.tabId<0 || pending.size>=500 || !['GET','POST','PUT','PATCH','DELETE','HEAD'].includes(details.method))return;
  // Register synchronously so completion cannot overtake async scope checks.
  pending.set(details.requestId,captureRequest(details));
}, {urls:['https://*/*','http://*/*'],types:['xmlhttprequest']}, ['requestBody']);
chrome.webRequest.onErrorOccurred.addListener(details=>pending.delete(details.requestId),{urls:['https://*/*','http://*/*'],types:['xmlhttprequest']});
chrome.webRequest.onCompleted.addListener(async details => {
  const capture=pending.get(details.requestId);pending.delete(details.requestId);
  const captured=await capture;if(!captured)return;
  const {observation,connection:original}=captured;
  try {
    const tab=await chrome.tabs.get(details.tabId);if(!tab.url||new URL(tab.url).origin!==observation.origin)return;
    const scope=`${observation.origin}:${tab.incognito?'private':'normal'}`;
    const connection=(await chrome.storage.local.get(`connection:${scope}`))[`connection:${scope}`];
    if(!connection || connection.token!==original.token || connection.storeId!==original.storeId || connection.incognito!==original.incognito || connection.siteOrigin!==observation.origin || !await chrome.permissions.contains({origins:connection.permissionOrigins || [observation.origin+'/*']}))return;
    const stores=await chrome.cookies.getAllCookieStores();
    if(!stores.some(store=>store.id===connection.storeId && store.tabIds.includes(tab.id)))return;
    if(observation.method==='GET'&&details.statusCode>=200&&details.statusCode<300&&!Object.keys(observation.query).length&&/(?:\/)(?:me|@me|viewer|userinfo|whoami|account|profile|session|current-user|current_user)\/?$/i.test(observation.path)){await chrome.storage.session.set({[`identity:${scope}`]:{url:observation.origin+observation.path,at:Date.now()}});}
    const {origin,...shape}=observation;shape.status=details.statusCode;
    const queue=queues.get(scope)||[];queue.push(shape);queues.set(scope,queue.slice(-40));
    if(timers.has(scope))return;
    timers.set(scope,setTimeout(async()=>{timers.delete(scope);const batch=queues.get(scope)||[];queues.delete(scope);try{const current=(await chrome.storage.local.get('connection:'+scope))['connection:'+scope];if(!current||current.token!==connection.token||current.storeId!==connection.storeId||current.siteOrigin!==connection.siteOrigin||current.incognito!==connection.incognito||!await chrome.permissions.contains({origins:connection.permissionOrigins||[connection.siteOrigin+'/*']}))return;const response=await fetch(connection.address+'/bridge/observe',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({token:connection.token,observations:batch}),credentials:'omit',redirect:'error'});if(response.status===403)await retireConnection(scope,connection);}catch{}},1500));
  } catch {}
},{urls:['https://*/*','http://*/*'],types:['xmlhttprequest']});

// Keep already-authorized website sessions current. Never request a new site permission here.
const refreshTimers = new Map();
async function refreshConnection(scope, connection) {
  if (!connection.storeId || !connection.siteOrigin) return;
  try {
    const site = new URL(connection.siteOrigin);
    const scopePlan = HYCLI_SITE_SCOPE.plan(site.origin);
    // Legacy connections retain their old exact-origin grant until reconnected.
    const permissionOrigins = connection.permissionOrigins || [site.origin+'/*'];
    if (!await chrome.permissions.contains({origins:permissionOrigins})) return;
    const cookies = await HYCLI_SITE_SCOPE.cookies(chrome,scopePlan,connection.storeId,navigator.userAgent.includes('Firefox/'),permissionOrigins);
    if (!cookies.length) return; // Visibility loss must not erase a known-good session.
    const current = (await chrome.storage.local.get('connection:'+scope))['connection:'+scope];
    if (!current || current.token !== connection.token || current.storeId !== connection.storeId || current.siteOrigin !== connection.siteOrigin || current.incognito !== connection.incognito || !await chrome.permissions.contains({origins:permissionOrigins})) return;
    const identity=(await chrome.storage.session.get('identity:'+scope))['identity:'+scope];
    const identity_source=identity && Date.now()-identity.at<30*60*1000?identity.url:'';
    const response=await fetch(connection.address+'/bridge/refresh',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({token:connection.token,cookies,identity_source}),credentials:'omit',redirect:'error'});
    if(response.status===403) await retireConnection(scope,connection);
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

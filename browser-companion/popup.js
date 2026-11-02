const $ = id => document.getElementById(id);
const supported = {'en':'English','ko':'한국어','ja':'日本語','zh-CN':'简体中文','zh-TW':'繁體中文','es':'Español','fr':'Français','de':'Deutsch','pt-BR':'Português','id':'Bahasa Indonesia','it':'Italiano','nl':'Nederlands','pl':'Polski','tr':'Türkçe','ru':'Русский','uk':'Українська','vi':'Tiếng Việt','th':'ไทย','ar':'العربية','hi':'हिन्दी'};
let locale = 'en';
function t(key) { return HYCLI_LOCALES[locale]?.[key] || HYCLI_LOCALES.en[key] || key; }
function translate() { document.documentElement.lang=locale;document.documentElement.dir=locale==='ar'?'rtl':'ltr';for(const el of document.querySelectorAll('[data-i18n]'))el.textContent=t(el.dataset.i18n);$('language').setAttribute('aria-label',t('settings.language'));$('address').setAttribute('aria-label',t('browser.address')); }
for(const [value,label] of Object.entries(supported)) $('language').add(new Option(label,value));
chrome.storage.local.get('locale').then(saved=>{const language=chrome.i18n.getUILanguage();locale=saved.locale in supported?saved.locale:language in supported?language:language.split('-')[0] in supported?language.split('-')[0]:'en';$('language').value=locale;translate();});
$('language').addEventListener('change',()=>{locale=$('language').value;translate();void chrome.storage.local.set({locale});});
translate();
let pairing = null;
let permissionScope = null;
const status = (message, error = false) => { $('status').textContent = message; $('status').className = error ? 'error' : ''; };
function address() {
  const value = new URL($('address').value);
  if (value.protocol !== 'http:' || value.hostname !== '127.0.0.1' || value.username || value.password || value.pathname !== '/' || value.search || value.hash) throw new Error(t('errors.bad_url'));
  return value.origin;
}
async function call(path, body) {
  let response;
  try { response = await fetch(address() + path, {method:'POST', headers:{'Content-Type':'application/json'},body:JSON.stringify(body),credentials:'omit',redirect:'error'}); }
  catch { throw new Error(t('errors.browser_unavailable')); }
  if (!response.ok) { const data = await response.json().catch(() => ({})); throw new Error(data.error?.code === 'pairing_expired' ? t('errors.pairing_expired') : t('errors.browser_unavailable')); }
  return response.json();
}
$('pair-form').addEventListener('submit', async event => {
  event.preventDefault(); $('pair').disabled = true; status(t('common.loading'));
  try { pairing = await call('/bridge/pair',{code:$('code').value}); permissionScope = HYCLI_SITE_SCOPE.plan(pairing.url,navigator.userAgent.includes('Firefox/')); $('website').value = permissionScope.origin; $('permission-origins').textContent = permissionScope.permissions.join('\n'); $('pair-form').hidden = true; $('connect-step').hidden = false; status(''); }
  catch (error) { status(error.message,true); } finally { $('pair').disabled = false; }
});
$('connect').addEventListener('click', async () => {
  // Permission requests remain directly in a user gesture.
  if (!pairing || !permissionScope) return;
  const scope = permissionScope;
  const origin = scope.origin;
  const permission = chrome.permissions.request({origins:scope.permissions});
  $('connect').disabled = true;
  try {
    if (!await permission) throw new Error(t('errors.forbidden'));
    const tabs = await chrome.tabs.query({active:true,currentWindow:true}); const tab = tabs[0];
    const stores = await chrome.cookies.getAllCookieStores(); const store = stores.find(s => s.tabIds.includes(tab?.id));
    if (!store) throw new Error(t('browser.signIn'));
    if (!tab?.url || new URL(tab.url).origin !== origin) throw new Error(t('browser.signIn'));
    const cookies = await HYCLI_SITE_SCOPE.cookies(chrome,scope,store.id,navigator.userAgent.includes('Firefox/'));
    // Empty can mean unavailable browser visibility, not proof of logout.
    if (!cookies.length) throw new Error(t('browser.cookiesUnavailable'));
    const current = (await chrome.tabs.query({active:true,currentWindow:true}))[0];
    const currentStores = await chrome.cookies.getAllCookieStores();
    if (current?.id !== tab.id || current.incognito !== tab.incognito || !current.url || new URL(current.url).origin !== origin || !currentStores.some(s=>s.id===store.id && s.tabIds.includes(current.id)) || !await chrome.permissions.contains({origins:scope.permissions})) throw new Error(t('errors.forbidden'));
    const key = `identity:${origin}:${tab.incognito?'private':'normal'}`;
    const observed = (await chrome.storage.session.get(key))[key];
    const identity_source = observed && Date.now()-observed.at < 30*60*1000 ? observed.url : '';
    status(t('common.loading'));
    const connected=await call('/bridge/connect',{token:pairing.token,cookies,browser:navigator.userAgent.includes('Edg/')?'Edge':navigator.userAgent.includes('Firefox/')?'Firefox':'Chrome',profile:$('profile').value,identity_source});
    await chrome.storage.local.set({[`connection:${origin}:${tab.incognito?'private':'normal'}`]:{address:address(),token:connected.session_token,storeId:store.id,siteOrigin:origin,permissionOrigins:scope.permissions,incognito:!!tab.incognito}});
    pairing = null; $('connect-step').hidden = true;
    status(identity_source ? t('accounts.connectedToast')+'\n'+t('accounts.checking') : t('accounts.connectedToast')+'\n'+t('accounts.unknownHint'));
  } catch (error) { status(error.message,true); } finally { $('connect').disabled=false; }
});

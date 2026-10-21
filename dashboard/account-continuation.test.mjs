import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import vm from 'node:vm';
const source = readFileSync(new URL('./src/dialogs.tsx', import.meta.url), 'utf8');
const start = source.indexOf('export function AccountDialog(');
const end = source.indexOf('  return <Modal', start);
const code = stripTypeScriptTypes(source.slice(start, end).replace('export ', '') + '\nreturn { resumeWith, scan, effects, currentTarget, continuePending, importAccount }; }');
const account = { id: 'account', site_id: 'site', site_url: 'https://example.test', status: 'connected', verified_identity: true };
function harness({ fresh = true, accounts = [account], act, found = null, waiting = false, boundAccount } = {}) {
  const calls = [], effects = [], states = [];
  const data = { accounts: [], sites: [{ id: 'site', url: 'https://example.test', account_id: boundAccount }], settings: { native_browser_discovery: true } };
  let snapshot = fresh ? { ...data, accounts } : null;
  const ops = { freshSnapshot: () => snapshot, act: async (...args) => { calls.push(args); return act ? act(...args) : {}; }, toast() {} };
  let stateIndex = 0;
  const context = { AbortController, effects, useI18n: () => ({t: x => x}), useState: value => [stateIndex++ === 6 ? found : stateIndex === 8 ? waiting : value, next => states.push(next)], useRef: current => ({current}), useEffect: fn => effects.push(fn), sameOrigin: (a,b) => new URL(a).origin === new URL(b).origin, codeOf: () => 'internal', request: () => Promise.resolve({profiles:[]}) };
  vm.createContext(context); vm.runInContext(code, context);
  const controller = context.AccountDialog({ site: data.sites[0], data, ops, onClose() {} });
  return { ...controller, calls, states, publish(value = {...data, accounts}) { snapshot = value; }, async reconcile() { controller.continuePending(); await new Promise(resolve => setImmediate(resolve)); } };
}
test('automatic continuation uses the published snapshot, not a closed-over account list', async () => {
  const h = harness(); await h.resumeWith('account'); assert.deepEqual(h.calls.map(c => c[0]), ['/api/browser/select', '/api/sites/prepare']);
});
test('stale snapshot waits for reconciliation without selecting or replaying', async () => {
  const h = harness({fresh:false}); await h.resumeWith('account'); assert.equal(h.calls.length, 0);
  h.publish(); await h.reconcile(); assert.equal(h.calls.filter(c=>c[0]==='/api/browser/select').length, 1);
  await h.reconcile(); assert.equal(h.calls.filter(c=>c[0]==='/api/browser/select').length, 1);
});
test('implicit selection does not gain cross-site or unverified authority', async () => {
  for (const changed of [{site_id:'other'}, {verified_identity:false}, {status:'expired'}]) {
    const h = harness({accounts:[{...account,...changed}]}); await h.resumeWith('account'); assert.equal(h.calls.length,0);
  }
});
test('a later explicit retry reselects its account after preparation failed and binding changed', async () => {
  const h = harness({act: async path => { if(path==='/api/sites/prepare') throw Error('prepare failed'); return {}; }});
  await assert.rejects(h.resumeWith('account', true));
  h.publish({accounts:[account], sites:[{id:'site',url:'https://example.test',account_id:'other-account'}], settings:{}});
  await assert.rejects(h.resumeWith('account', true));
  assert.deepEqual(h.calls.map(c=>c[0]), ['/api/browser/select','/api/sites/prepare','/api/browser/select','/api/sites/prepare']);
  assert.equal(h.calls[2][2].account_id, 'account');
});
test('failed preparation never causes automatic selection replay', async () => {
  const h = harness({act: async path => { if(path==='/api/sites/prepare') throw Error('prepare failed'); return {}; }});
  await assert.rejects(h.resumeWith('account')); await h.resumeWith('account'); await h.reconcile();
  assert.equal(h.calls.filter(c=>c[0]==='/api/browser/select').length,1);
});
test('target change after accepted selection prevents old target continuation', async () => {
  let accept; const h=harness({act: path=>path==='/api/browser/select'?new Promise(r=>accept=r):{}});
  const pending=h.resumeWith('account',true); h.currentTarget.current='changed'; accept({}); await pending;
  assert.equal(h.calls.length,1);
});
test('automatic rejected or ambiguous selection is not replayed on reconciliation', async () => {
  const h=harness({act: async () => {throw Error('network or rejection');}});
  await assert.rejects(h.resumeWith('account')); await h.resumeWith('account'); await h.reconcile();
  assert.equal(h.calls.length,1);
});
test('import receipt is retained while snapshot synchronization is unavailable', async () => {
  const h=harness({fresh:false,act: async () => ({account})}); await h.importAccount('synthetic cookies');
  assert.equal(h.calls.length,1); assert.equal(h.calls[0][0],'/api/accounts/import');
  assert.ok(h.states.includes(account));
  h.publish(); await h.reconcile(); assert.equal(h.calls.filter(c=>c[0]==='/api/browser/select').length,1);
});
test('changing the target cancels a pending continuation', async () => {
  const h=harness({fresh:false}); await h.resumeWith('account'); h.currentTarget.current='changed';
  h.publish(); await h.reconcile(); assert.equal(h.calls.length,0);
});
test('needs_choice prevents automatic found selection, but not an existing site binding', async () => {
  for (const boundAccount of [undefined, 'account']) {
    const h=harness({waiting:true,found:{selected_account:'account',needs_choice:true},boundAccount});
    h.effects.find(fn=>String(fn).includes('const candidates'))(); await new Promise(resolve=>setImmediate(resolve));
    assert.equal(h.calls.filter(c=>c[0]==='/api/browser/select').length,boundAccount?1:0);
  }
});
test('closing while selection is pending invalidates its continuation', async () => {
  let accept; const h=harness({act:path=>path==='/api/browser/select'?new Promise(r=>accept=r):{}});
  const unmount=h.effects.find(fn=>String(fn).includes('currentTarget.current = targetKey'))();
  const pending=h.resumeWith('account',true); unmount(); accept({}); await pending;
  assert.equal(h.calls.length,1);
});
test('closing while import is pending preserves acceptance but does not select', async () => {
  let accept; const h=harness({act:()=>new Promise(r=>accept=r)});
  const unmount=h.effects.find(fn=>String(fn).includes('currentTarget.current = targetKey'))();
  const pending=h.importAccount('synthetic'); unmount(); accept({account}); await pending;
  assert.equal(h.calls.length,1); assert.ok(!h.states.includes(account));
});
test('a later needs_choice receipt cancels an older queued implicit selection', async () => {
  const h = harness({ fresh: false, act: async () => ({ needs_choice: true, selected_account: '', accounts: [account] }) });
  await h.resumeWith('account');
  await h.scan(true);
  h.publish(); await h.reconcile();
  assert.deepEqual(h.calls.map(c => c[0]), ['/api/browser/find']);
});
test('native discovery cancellation owns its target lifetime and ignores its own abort', async () => {
 let signal;
 const h=harness({act:async(path,method,body,currentSignal)=>{signal=currentSignal;return new Promise((resolve,reject)=>signal.addEventListener('abort',()=>reject(new DOMException('Cancelled','AbortError')),{once:true}));}});
 const cleanup=h.effects.find(fn=>String(fn).includes('nativeRequest.current?.abort'))();
 const pending=h.scan();assert.equal(signal.aborted,false);cleanup();await pending;
 assert.equal(signal.aborted,true);assert.ok(!h.states.includes('internal'));
 assert.deepEqual(h.calls.map(c=>c[0]),['/api/browser/find']);
});

const english = JSON.parse(readFileSync(new URL('./src/locales/en.json', import.meta.url), 'utf8'));
const summaryExpression = source.slice(source.indexOf("{t('accounts.searchDone'")).match(/<p>\{t\(([\s\S]*?)\)\}<\/p>/)[1];
function discoverySummary(accounts, { needs_choice = false, selected_account = '' } = {}) {
  const found = { profiles_checked: 1, profiles_unavailable: 0, accounts, selected_account, needs_choice, login_url: 'https://example.test' };
  return vm.runInNewContext(`t(${summaryExpression})`, { found, t: key => english[key] });
}
test('discovery confirms a selected verified connected account without asking for verification', () => {
  assert.equal(discoverySummary([account], { selected_account: account.id }), 'Account connected');
});
test('discovery retains the account-choice prompt even when accounts are verified', () => {
  assert.equal(discoverySummary([account, { ...account, id: 'second' }], { needs_choice: true }), 'More than one account was found. Choose the account to use.');
});
test('discovery still explains verification for connected unverified sessions', () => {
  assert.equal(discoverySummary([{ ...account, verified_identity: false }]), 'Browser session found. Your account still needs to be verified.');
});
test('discovery does not present expired accounts as usable sessions', () => {
  for (const verified_identity of [false, true]) {
    assert.equal(discoverySummary([{ ...account, status: 'expired', verified_identity }], { selected_account: account.id }), 'No usable sign-in was found. Open the website, sign in, then return here.');
  }
});
test('discovery explains how to sign in when no session was found', () => {
  assert.equal(discoverySummary([]), 'No usable sign-in was found. Open the website, sign in, then return here.');
});
test('mixed discovery results confirm one connected account without claiming all are verified', () => {
  const others = [{ ...account, id: 'unverified', verified_identity: false }, { ...account, id: 'expired', status: 'expired' }];
  for (const accounts of [[account, ...others], [...others, account]]) {
    assert.equal(discoverySummary(accounts, { selected_account: account.id }), 'Account connected');
  }
});
test('an expired verified account does not hide a connected session that needs verification', () => {
  assert.equal(discoverySummary([{ ...account, status: 'expired' }, { ...account, id: 'unverified', verified_identity: false }]), 'Browser session found. Your account still needs to be verified.');
});

const actionDialog = source.slice(source.indexOf('export function ActionDialog('), source.indexOf('export function ApprovalDialog('));
const verificationLabel = actionDialog.match(/\{([^{}]+) && <span>\{(t\('actions\.unverified'\))\}<\/span>\}/);
assert.ok(verificationLabel, 'the action dialog exposes its unverified status label');
for (const [state, verified] of [['never attempted', false], ['attempted with an unverified response', false], ['verified response', true]]) {
  test(`action verification label is truthful for ${state}`, () => {
    const label = vm.runInNewContext(`${verificationLabel[1]} && ${verificationLabel[2]}`, {
      action: { verified }, t: key => english[key],
    });
    assert.equal(label, verified ? false : 'Not verified');
  });
}

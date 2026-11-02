import { readFileSync } from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import assert from 'node:assert/strict';
import { test } from 'node:test';
const source = stripTypeScriptTypes(readFileSync(new URL('./src/snapshot.ts', import.meta.url), 'utf8'));
const { snapshotCoordinator } = await import(`data:text/javascript,${encodeURIComponent(source)}`);
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
test('accepted receipt survives failed or hanging snapshot without replay', async () => {
  for (const fails of [true, false]) {
    let posts = 0; const pending = deferred(); const status = [];
    const sync = snapshotCoordinator(() => pending.promise, () => {}, x => status.push(x));
    const receipt = { id: 'accepted' };
    assert.equal(await sync.act(async () => { posts++; return receipt; }), receipt);
    assert.equal(sync.freshSnapshot(), null);
    if (fails) { pending.reject(new Error('offline')); await new Promise(setImmediate); }
    assert.equal(posts, 1); assert.equal(status.at(-1), false);
  }
});
test('rejected or ambiguous command stays rejected and is not replayed', async () => {
  let reads = 0; let posts = 0;
  const sync = snapshotCoordinator(async () => { reads++; }, () => {}, () => {});
  for (const error of [new Error('rejected'), new TypeError('network')]) {
    await assert.rejects(sync.act(async () => { posts++; throw error; }), e => e === error);
  }
  assert.equal(posts, 2); assert.equal(reads, 0);
});
test('pre-command and older reads cannot overwrite or certify current state', async () => {
  const reads = []; const values = []; const status = [];
  const sync = snapshotCoordinator(() => { const p = deferred(); reads.push(p); return p.promise; }, x => values.push(x), x => status.push(x));
  const before = sync.refresh(); await sync.act(async () => 'accepted');
  reads[0].resolve('before'); assert.equal(await before, null); assert.equal(sync.freshSnapshot(), null);
  const newest = sync.refresh(); reads[2].resolve('newest'); assert.equal(await newest, 'newest');
  reads[1].reject(new Error('older failure')); await new Promise(setImmediate);
  assert.deepEqual(values, ['newest']); assert.equal(sync.freshSnapshot(), 'newest'); assert.equal(status.at(-1), true);
  const older = sync.refresh(); const later = sync.refresh(); reads[4].resolve('later'); await later;
  reads[3].resolve('older'); assert.equal(await older, null); assert.deepEqual(values, ['newest', 'later']);
});

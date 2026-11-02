import assert from 'node:assert/strict';

export const expectedFixtures = ['apex_host', 'parent_domain', 'partitioned_parent', 'unrelated', 'www_domain', 'www_host'];
const names = cookies => cookies.map(c => c.name).sort();
export function verifyVisibility(label, result) {
  assert.deepEqual(names(result.seeded), expectedFixtures, 'All synthetic fixtures must exist before querying');
  for (const cookies of [result.all, result.www, result.store, result.partitioned]) {
    assert.ok(Array.isArray(cookies), 'Native API errors must fail, not become empty cookie lists');
    assert.ok(!cookies.some(c => c.name === 'unrelated'), 'Unrelated origin must remain inaccessible');
  }
  assert.deepEqual(names(result.all), names(result.store), 'Explicit normal store must match default store');
  for (const key of ['all', 'www', 'store']) {
    assert.ok(result[key].every(c => !c.partitionKey), `${key} query must exclude partitioned cookies without a partition key`);
  }
  assert.ok(result.www.every(c => c.name !== 'apex_host'), 'Apex host-only cookie must not match www URL');
  assert.ok(result.partitioned.every(c => c.name === 'partitioned_parent' && c.partitionKey?.topLevelSite === 'https://top-proof.test'));
  if (label === 'none') {
    for (const key of ['all', 'www', 'store', 'partitioned']) assert.deepEqual(result[key], [], `No host permission: ${key}`);
    return 'no-host-permission';
  }
  assert.ok(result.all.some(c => c.name === 'www_host' && c.hostOnly === true));
  assert.ok(result.all.some(c => c.name === 'www_domain' && c.hostOnly === false));
  assert.ok(result.www.some(c => c.name === 'www_host' && c.hostOnly === true));
  assert.ok(result.www.some(c => c.name === 'www_domain' && c.hostOnly === false));
  assert.ok(result.www.every(c => ['www_host', 'www_domain', 'parent_domain'].includes(c.name)));
  if (label === 'both') {
    assert.deepEqual(names(result.all), ['apex_host', 'parent_domain', 'www_domain', 'www_host']);
    assert.deepEqual(names(result.www), ['parent_domain', 'www_domain', 'www_host']);
    assert.deepEqual(names(result.partitioned), ['partitioned_parent']);
  } else {
    assert.equal(label, 'www');
    assert.ok(!result.all.some(c => c.name === 'apex_host'));
    assert.ok(result.all.every(c => ['www_host', 'www_domain', 'parent_domain'].includes(c.name)));
  }
  // This is the measured browser difference, not an assumption that every engine
  // requires an apex grant. Both outcomes are retained with the browser version.
  return result.all.some(c => c.name === 'parent_domain') ? 'parent-visible-with-current-grants' : 'parent-filtered-with-current-grants';
}

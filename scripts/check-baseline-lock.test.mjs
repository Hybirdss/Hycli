import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const checker = path.join(root, 'scripts/check-baseline-lock.mjs');

async function fixture(t) {
  const scratch = path.join(root, '.cache/baseline-lock-tests');
  await fs.mkdir(scratch, {recursive: true});
  const directory = await fs.mkdtemp(path.join(scratch, 'version with spaces-'));
  t.after(() => fs.rm(directory, {recursive: true, force: true}));
  const baseline = {
    name: 'dashboard', version: '0.1.0', lockfileVersion: 3, requires: true,
    packages: {
      '': {name: 'dashboard', version: '0.1.0', dependencies: {react: '19.3.0'}},
      'node_modules/react': {version: '19.3.0', resolved: 'https://example.invalid/react.tgz', integrity: 'sha512-example'}
    }
  };
  const current = structuredClone(baseline);
  current.version = '0.1.1';
  current.packages[''].version = '0.1.1';
  for (const [name, lock] of Object.entries({current, baseline})) {
    await fs.mkdir(path.join(directory, name));
    await fs.writeFile(path.join(directory, name, 'package.json'), JSON.stringify({name: 'dashboard', version: lock.version}));
    await fs.writeFile(path.join(directory, name, 'package-lock.json'), JSON.stringify(lock));
  }
  return {directory, current, baseline};
}

async function run(directory) {
  const files = ['current/package.json', 'current/package-lock.json', 'baseline/package.json', 'baseline/package-lock.json'];
  const before = await Promise.all(files.map(file => fs.readFile(path.join(directory, file), 'utf8')));
  const result = spawnSync(process.execPath, [checker, path.join(directory, 'current'), path.join(directory, 'baseline')], {encoding: 'utf8'});
  assert.deepEqual(await Promise.all(files.map(file => fs.readFile(path.join(directory, file), 'utf8'))), before, 'the check must not change either source');
  return result;
}

test('allows only a consistent project version bump without editing either checkout', async t => {
  const {directory} = await fixture(t);
  const result = await run(directory);
  assert.equal(result.status, 0, result.stderr);
});

test('accepts identical lockfiles and irrelevant JSON key order', async t => {
  const {directory, baseline} = await fixture(t);
  await fs.writeFile(path.join(directory, 'current/package.json'), JSON.stringify({name: 'dashboard', version: '0.1.0'}));
  await fs.writeFile(path.join(directory, 'current/package-lock.json'), JSON.stringify(Object.fromEntries(Object.entries(baseline).reverse()), null, 2));
  const result = await run(directory);
  assert.equal(result.status, 0, result.stderr);
});

const changes = {
  'dependency version': lock => { lock.packages['node_modules/react'].version = '19.4.0'; },
  'dependency resolution': lock => { lock.packages['node_modules/react'].resolved += '?other'; },
  'dependency integrity': lock => { lock.packages['node_modules/react'].integrity = 'sha512-other'; },
  'root dependency constraint': lock => { lock.packages[''].dependencies.react = '19.4.0'; },
  'top-level package name': lock => { lock.name = 'another-dashboard'; },
  'lockfile format': lock => { lock.lockfileVersion = 2; },
  'unexpected root field': lock => { lock.packages[''].hasInstallScript = true; },
  'inconsistent top-level version': lock => { lock.version = '0.2.0'; },
  'inconsistent root version': lock => { lock.packages[''].version = '0.2.0'; },
  'missing root version': lock => { delete lock.packages[''].version; },
  'missing package root': lock => { delete lock.packages['']; }
};
for (const [name, change] of Object.entries(changes)) {
  test(`rejects ${name}`, async t => {
    const {directory, current} = await fixture(t);
    change(current);
    await fs.writeFile(path.join(directory, 'current/package-lock.json'), JSON.stringify(current));
    assert.notEqual((await run(directory)).status, 0);
  });
}

for (const side of ['current', 'baseline']) {
  test(`rejects missing or empty versions in ${side}`, async t => {
    const {directory} = await fixture(t);
    for (const version of [undefined, '']) {
      await fs.writeFile(path.join(directory, side, 'package.json'), JSON.stringify({version}));
      await fs.writeFile(path.join(directory, side, 'package-lock.json'), JSON.stringify({version, packages: {'': {version}}}));
      assert.notEqual((await run(directory)).status, 0);
    }
  });
}

test('rejects an added or removed dependency even after a legitimate version bump', async t => {
  const {directory, current} = await fixture(t);
  const added = structuredClone(current);
  added.packages['node_modules/extra'] = {version: '0.1.0'};
  const removed = structuredClone(current);
  delete removed.packages['node_modules/react'];
  for (const lock of [added, removed]) {
    await fs.writeFile(path.join(directory, 'current/package-lock.json'), JSON.stringify(lock));
    assert.notEqual((await run(directory)).status, 0);
  }
});

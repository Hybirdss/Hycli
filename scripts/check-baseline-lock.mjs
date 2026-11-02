import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';

async function dependencyLock(directory) {
  const manifest = JSON.parse(await fs.readFile(path.join(directory, 'package.json'), 'utf8'));
  const lock = JSON.parse(await fs.readFile(path.join(directory, 'package-lock.json'), 'utf8'));
  assert.equal(typeof manifest.version, 'string', `${directory}: missing package version`);
  assert.ok(manifest.version.length > 0, `${directory}: empty package version`);
  assert.equal(lock.version, manifest.version, `${directory}: lockfile version must match package.json`);
  assert.equal(lock.packages?.['']?.version, manifest.version, `${directory}: lockfile root version must match package.json`);
  delete lock.version;
  delete lock.packages[''].version;
  return lock;
}

const directories = process.argv.slice(2);
assert.equal(directories.length, 2, 'Usage: node scripts/check-baseline-lock.mjs <current dashboard> <baseline dashboard>');
const [current, baseline] = await Promise.all(directories.map(dependencyLock));
assert.deepEqual(current, baseline, 'Baseline lockfiles must match except for their validated project versions');

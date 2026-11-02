import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { documentationScreenshot } from './e2e-screenshots.mjs';

test('documentation captures honor opt-out without changing existing or creating new assets', async t => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'hycli-screenshots-'));
  const previous = process.env.HYCLI_NO_SCREENSHOTS;
  t.after(async () => {
    if (previous === undefined) delete process.env.HYCLI_NO_SCREENSHOTS;
    else process.env.HYCLI_NO_SCREENSHOTS = previous;
    await fs.rm(directory, { recursive: true, force: true });
  });
  const existing = path.join(directory, 'ai-connections.png');
  const missing = path.join(directory, 'browser-companion-actual.png');
  const reviewed = Buffer.from('reviewed screenshot bytes');
  const generated = Buffer.from('regenerated screenshot bytes');
  await fs.writeFile(existing, reviewed);
  const captures = [];
  const page = { async screenshot(options) {
    captures.push(options);
    await fs.writeFile(options.path, generated);
    return generated;
  } };

  process.env.HYCLI_NO_SCREENSHOTS = '1';
  for (const destination of [existing, missing]) {
    assert.equal(await documentationScreenshot(page, { path: destination }), undefined);
  }
  await documentationScreenshot(page, () => assert.fail('Disabled captures must skip screenshot-only preparation'));
  assert.deepEqual(captures, []);
  assert.deepEqual(await fs.readFile(existing), reviewed);
  await assert.rejects(fs.access(missing), { code: 'ENOENT' });

  // Both direct-suite defaults and the aggregate runner's explicit opt-in work.
  for (const value of [undefined, '0']) {
    if (value === undefined) delete process.env.HYCLI_NO_SCREENSHOTS;
    else process.env.HYCLI_NO_SCREENSHOTS = value;
    const options = { path: existing, fullPage: true, timeout: 120000 };
    assert.deepEqual(await documentationScreenshot(page, options), generated);
    const crop = { path: missing, clip: { x: 0, y: 0, width: 390, height: 844 } };
    let prepared = 0;
    assert.deepEqual(await documentationScreenshot(page, async () => { prepared++; return crop; }), generated);
    assert.equal(prepared, 1);
    assert.deepEqual(captures.slice(-2), [
      { animations: 'disabled', ...options },
      { animations: 'disabled', ...crop },
    ]);
    assert.deepEqual(await fs.readFile(existing), generated);
    assert.deepEqual(await fs.readFile(missing), generated);
  }
  await assert.rejects(documentationScreenshot({ screenshot() { throw Error('capture failed'); } }, {}), /capture failed/);
});

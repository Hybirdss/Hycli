// Pure dummy browser-object contract. No browser, file opening, or HTTP navigation.
import { readFileSync, readdirSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import assert from 'node:assert/strict';
const source = readFileSync(new URL('../src/dashboard/bootstrap.js', import.meta.url), 'utf8');
const tick = () => new Promise(resolve => setImmediate(resolve));
function run(hash, response = { ok: true }, language = 'en') {
  const calls = []; const status = { textContent: '' }; const documentElement = {};
  runInNewContext(source, {
    URLSearchParams, location: { hash, replace: path => calls.push(['replace', path]) },
    history: { replaceState: (...args) => calls.push(['erase', args[2]]) },
    navigator: { get language() { assert.equal(calls[0]?.[0], 'erase'); return language; } },
    document: { documentElement, getElementById: () => status },
    fetch: (path, options) => { calls.push(['fetch', path, options]); return response instanceof Error ? Promise.reject(response) : Promise.resolve(response); },
  });
  return { calls, status, documentElement };
}
test('dummy fragment erased before same-origin POST and successful replace', async () => {
  const ticket = 'a'.repeat(64); const { calls } = run(`#ticket=${ticket}`); await tick();
  assert.deepEqual(calls.map(c => c[0]), ['erase', 'fetch', 'replace']);
  assert.equal(calls[0][1], '/bootstrap'); assert.equal(calls[1][1], '/api/bootstrap');
  assert.equal(calls[1][2].method, 'POST'); assert.equal(calls[1][2].credentials, 'same-origin');
  assert.equal(calls[1][2].cache, 'no-store');
  assert.equal(calls[1][2].headers['Content-Type'], 'application/json');
  assert.deepEqual(JSON.parse(calls[1][2].body), { ticket }); assert.equal(calls[2][1], '/');
});
test('missing or invalid dummy input never requests authority', async () => {
  for (const hash of ['', '#ticket=invalid', '#ticket=', '#ticket=' + 'a'.repeat(63), '#ticket=' + 'a'.repeat(65), '#ticket=' + 'g'.repeat(64)]) { const { calls, status } = run(hash); await tick(); assert.equal(calls.length, 1); assert.match(status.textContent, /Open Hycli again/); }
});
test('network or authorization failure has no fallback secret URL or retry', async () => {
  for (const response of [{ ok: false }, new Error('synthetic offline')]) { const { calls, status } = run('#ticket=' + 'b'.repeat(64), response); await tick(); await tick(); assert.deepEqual(calls.map(c => c[0]), ['erase', 'fetch']); assert.match(status.textContent, /Open Hycli again/); }
});

test('all existing dashboard locales have self-contained opening and recovery text', async () => {
  const locales = readdirSync(new URL('./src/locales/', import.meta.url)).map(file => file.replace(/\.json$/, ''));
  assert.equal(locales.length, 20);
  for (const locale of locales) {
    const { calls, status, documentElement } = run('#ticket=' + 'c'.repeat(64), { ok: false }, locale);
    const opening = status.textContent;
    assert.equal(documentElement.lang, locale);
    assert.equal(documentElement.dir, locale === 'ar' ? 'rtl' : 'ltr');
    assert.match(opening, /Hycli/);
    if (locale !== 'en') assert.notEqual(opening, 'Opening Hycli…');
    await tick();
    assert.match(status.textContent, /Hycli/);
    assert.notEqual(status.textContent, opening);
    if (locale !== 'en') assert.notEqual(status.textContent, 'Open Hycli again to use this window.');
    assert.deepEqual(calls.map(c => c[0]), ['erase', 'fetch']);
  }
});
test('browser language uses supported exact, regional and English fallback locales', () => {
  for (const [language, expected] of [
    ['KO-kr', 'ko'], ['fr-CA', 'fr'], ['pt-PT', 'pt-BR'], ['zh', 'zh-CN'],
    ['zh-SG', 'zh-CN'], ['zh-Hans-CN', 'zh-CN'], ['zh-Hant', 'zh-TW'],
    ['zh-Hant-HK', 'zh-TW'], ['zh-HK', 'zh-TW'], ['zh-MO', 'zh-TW'],
    ['en-US', 'en'], ['xx-ZZ', 'en'], ['', 'en'], ['constructor', 'en'],
  ]) {
    const { calls, status, documentElement } = run('', { ok: true }, language);
    assert.equal(documentElement.lang, expected, language);
    assert.deepEqual(calls.map(c => c[0]), ['erase']);
    assert.ok(status.textContent);
  }
});

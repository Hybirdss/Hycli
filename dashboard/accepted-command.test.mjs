import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
import { stripTypeScriptTypes } from 'node:module';
import { test } from 'node:test';
const app = fs.readFileSync(new URL('./src/App.tsx', import.meta.url), 'utf8');
const api = fs.readFileSync(new URL('./src/api.ts', import.meta.url), 'utf8').replaceAll('export ', '');
const coordinator = fs.readFileSync(new URL('./src/snapshot.ts', import.meta.url), 'utf8').replaceAll('export ', '');
const start = app.indexOf('  const submit =', app.indexOf('function Websites('));
const submit = app.slice(start, app.indexOf('\n  return <>', start));
for (const mode of ['failure', 'hang', 'edited']) test(`production website submit accepts receipt with ${mode} refresh`, async () => {
  let error = '', busy = false, posts = 0, fresh = true, resolvePost;
  const draft = { url: 'https://fixture.example.test', intent: 'Read report' };
  const context = { URL, URLSearchParams, Response, location: { search: '' }, provider: 'fixture', draftRevision: { current: 0 }, ...draft,
    setError: v => error = v, setBusy: v => busy = v, setURL: v => draft.url = v, setIntent: v => draft.intent = v,
    setFresh: v => fresh = v,
    fetch: async (path, options) => {
      if (path === '/api/sites/prepare') { posts++; if (mode === 'edited') await new Promise(resolve => resolvePost = resolve); return new Response(JSON.stringify({ job: { id: 'accepted' } }), { status: 200 }); }
      assert.equal(path, '/api/state');
      if (mode === 'hang') return new Promise(() => {});
      throw new Error('synthetic outage');
    },
  };
  vm.createContext(context);
  vm.runInContext(stripTypeScriptTypes(`${api}\n${coordinator}\nconst sync=snapshotCoordinator(()=>request('/api/state'),()=>{},setFresh); const ops={act:(path, method, body)=>sync.act(()=>request(path,method,body))}; ${submit}\nglobalThis.submit=submit;`, { mode: 'transform' }), context);
  const pending = context.submit({ preventDefault() {} });
  if (mode === 'edited') {
    context.draftRevision.current++;
    draft.url = 'https://edited.example.test'; draft.intent = 'New draft';
    resolvePost();
  }
  await pending;
  assert.equal(posts, 1); assert.equal(error, ''); assert.equal(busy, false); assert.equal(fresh, false);
  assert.equal(draft.url, mode === 'edited' ? 'https://edited.example.test' : '');
  assert.equal(draft.intent, mode === 'edited' ? 'New draft' : '');
});
test('successful current publication clears only Workspace sync error and retires resolved approval receipts', () => {
  const start = app.indexOf('    next => {', app.indexOf('snapshotCoordinator('));
  const end = app.indexOf('\n    },', start);
  const publish = app.slice(start, end) + '\n    }';
  let workspaceError = 'connection_lost', formError = 'ai_unavailable';
  let resolved = new Set(['still-pending', 'resolved', 'removed']);
  const next = { approvals: [{ id: 'still-pending', status: 'pending' }, { id: 'resolved', status: 'approved' }] };
  const context = { snapshotRef: { current: {} }, setData() {}, setError: value => workspaceError = value, setResolvedApprovals: update => resolved = update(resolved) };
  vm.createContext(context); vm.runInContext(`globalThis.publish = ${publish}`, context);
  context.publish(next);
  assert.equal(workspaceError, ''); assert.equal(formError, 'ai_unavailable');
  assert.deepEqual([...resolved], ['still-pending']);
});

function artifactApi(fetch, overrides = {}) {
  const links = [];
  const context = { URL, URLSearchParams, Response, location: { search: '' }, history: {},
    document: { body: { appendChild() {} }, createElement: () => {
      const link = { click() { links.push({ href: link.href, download: link.download }); }, remove() {} };
      return link;
    } }, fetch, ...overrides };
  vm.createContext(context);
  vm.runInContext(stripTypeScriptTypes(`${api}\nglobalThis.api = { checkArtifact, downloadArtifact };`, { mode: 'transform' }), context);
  return { ...context.api, links };
}
test('native artifact download uses cookie HEAD and a same-origin link, never a buffered GET', async () => {
  const reads = [];
  const client = artifactApi(async (path, options) => {
    reads.push({ path, options });
    const response = new Response(null, { status: 200 });
    response.blob = () => { throw Error('file must not be buffered'); };
    return response;
  });
  const signal = new AbortController().signal;
  await client.downloadArtifact('job/one', 'artifact?two', 'same-name.bin', signal);
  assert.equal(reads.length, 1);
  assert.equal(reads[0].options.method, 'HEAD');
  assert.equal(reads[0].options.credentials, 'same-origin');
  assert.equal(reads[0].options.signal, signal);
  assert.deepEqual(client.links, [{ href: '/api/jobs/job%2Fone/artifacts/artifact%3Ftwo', download: 'same-name.bin' }]);
});
test('HEAD 401 renews existing session once, without requiring a nonexistent error body', async () => {
  const calls = [];
  const client = artifactApi(async (path, options) => {
    calls.push({ path, options });
    if (path === '/api/session') return new Response(JSON.stringify({ csrf: 'new-session' }));
    return new Response(null, { status: calls.length === 1 ? 401 : 200 });
  });
  await client.downloadArtifact('job', 'file', 'file.bin', new AbortController().signal);
  assert.deepEqual(calls.map(c => c.path), ['/api/jobs/job/artifacts/file', '/api/session', '/api/jobs/job/artifacts/file']);
  assert.ok(calls.every(c => c.options.credentials === 'same-origin'));
  assert.equal(client.links.length, 1);
});
test('missing, malformed and unreachable artifact checks never start a download or rerun an action', async () => {
  for (const [status, code] of [[404, 'not_found'], [400, 'data_invalid'], [409, 'data_invalid'], [500, 'internal'], [401, 'session_expired']]) {
    const client = artifactApi(async path => path === '/api/session' ? new Response(JSON.stringify({ csrf: 'session' })) : new Response(null, { status }));
    await assert.rejects(client.downloadArtifact('job', 'file', 'file.bin', new AbortController().signal), e => e.code === code);
    assert.equal(client.links.length, 0);
  }
  const client = artifactApi(async () => { throw Error('offline'); });
  await assert.rejects(client.checkArtifact('job', 'file', new AbortController().signal), e => e.code === 'connection_lost');
});
test('closing a file view during its availability check cannot launch a delayed native download', async () => {
  let release;
  const client = artifactApi(() => new Promise(resolve => { release = resolve; }));
  const controller = new AbortController();
  const pending = client.downloadArtifact('job', 'file', 'file.bin', controller.signal);
  controller.abort(); release(new Response(null));
  await assert.rejects(pending, e => e.name === 'AbortError');
  assert.equal(client.links.length, 0);
});
test('file inventory uses only completed action artifacts and preserves repeated filenames by identity', () => {
  const fileSource = fs.readFileSync(new URL('./src/file-result.tsx', import.meta.url), 'utf8');
  const parse = fileSource.slice(fileSource.indexOf('export function fileArtifact'), fileSource.indexOf('type FileAccess'));
  const list = fileSource.slice(fileSource.indexOf('export function completedFiles'), fileSource.indexOf('export function FileHistory'));
  const context = {}; vm.createContext(context);
  vm.runInContext(stripTypeScriptTypes(`${parse}\n${list}\nglobalThis.completedFiles=completedFiles;`.replaceAll('export ', '')), context);
  const artifact = { kind: 'file', version: 1, status: 'complete', location_scope: 'hycli_host', content_trust: 'untrusted', artifact_id: 'a', filename: 'same.bin', local_path: '/host/a', media_type: 'application/octet-stream', byte_count: 10, sha256: 'a'.repeat(64), created_at: '2026-10-20T10:00:00Z', provenance: { site: 'example', action: 'export', source_origin: 'https://example.test' } };
  const jobs = [
    { id: 'first', kind: 'action', status: 'completed', artifact },
    { id: 'second', kind: 'action', status: 'completed', artifact: { ...artifact, artifact_id: 'b', local_path: '/host/b' } },
    { id: 'json', kind: 'action', status: 'completed', result: artifact },
    { id: 'running', kind: 'action', status: 'running', artifact },
    { id: 'failed', kind: 'action', status: 'failed', artifact },
    { id: 'summary', kind: 'summary', status: 'completed', artifact },
  ];
  const listed = context.completedFiles(jobs);
  assert.deepEqual(Array.from(listed, row => [row.job.id, row.file.artifact_id, row.file.filename]), [['first', 'a', 'same.bin'], ['second', 'b', 'same.bin']]);
  assert.equal(context.completedFiles(jobs.slice(2)).length, 0, 'Pruned history is not silently rebuilt from results or paths');
});

#!/usr/bin/env node
// Actual process-boundary regression, using only an owned loopback HTTP fixture.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import http from 'node:http';
import {spawn} from 'node:child_process';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const repository = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const target = path.resolve(repository, process.env.CARGO_TARGET_DIR || 'target');
const binary = process.env.HYCLI_BIN || path.join(target,
  ...(process.env.CARGO_BUILD_TARGET ? [process.env.CARGO_BUILD_TARGET] : []),
  'debug', process.platform === 'win32' ? 'hycli.exe' : 'hycli');
const scratch = path.resolve(process.env.HYCLI_TEST_DATA || path.join(repository, '.cache'));
await fs.mkdir(scratch, {recursive: true});
const root = await fs.mkdtemp(path.join(scratch, 'http-denial-'));
const canaries = ['synthetic-private-body-8317', 'synthetic-private-message-9428',
  'synthetic-private-token-1653', 'synthetic-private-request-id-2764'];
const privateFields = {body: canaries[0], message: canaries[1], token: canaries[2], request_id: canaries[3]};
const cases = [
  {name: 'plain-denied', status: 403, code: 'api_denied', defense: 'none'},
  {name: 'known-denied', status: 403, code: 'api_denied', defense: 'none',
    body: {error: {code: 'ACCESS_DENIED', ...privateFields}}, reason: 'access_denied'},
  {name: 'unknown-denied', status: 403, code: 'api_denied', defense: 'none',
    body: {code: 'forbidden-' + canaries[0], error: {type: canaries[2], message: 'permission_denied'}, ...privateFields}},
  {name: 'projected-denied', status: 403, code: 'api_denied', defense: 'none', projected: true,
    body: {...privateFields, nextPageToken: 'x', x: canaries[0]}},
  {name: 'unauthorized', status: 401, code: 'auth_required', defense: 'none', exit: 3},
  {name: 'missing', status: 404, code: 'request_failed', defense: 'none', exit: 1},
  {name: 'server-error', status: 500, code: 'request_failed', defense: 'none', exit: 1},
  {name: 'challenge', status: 403, code: 'blocked_by_site', defense: 'cloudflare',
    headers: {'cf-mitigated': 'challenge'}},
  {name: 'rate-limit', status: 429, code: 'site_rate_limited', defense: 'rate-limit'},
];
let active;
let requests = [];
const server = http.createServer((req, res) => {
  requests.push(req.url);
  res.setHeader('content-type', 'application/json');
  if (req.url === '/healthy') return res.end(JSON.stringify({ok: true}));
  if (req.url !== '/failure') { res.statusCode = 404; return res.end('{}'); }
  res.statusCode = active.status;
  for (const [key, value] of Object.entries(active.headers || {})) res.setHeader(key, value);
  res.end(JSON.stringify(active.body || privateFields));
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const site = 'http-denial-fixture';
const spec = {spec_version: 2, site: {name: site, base_url: `http://127.0.0.1:${server.address().port}`},
  operations: ['failure', 'healthy'].map(name => ({name, method: 'GET', path: '/' + name,
    effect: 'read', evidence: 'Owned synthetic HTTP denial regression fixture'}))};
const file = path.join(root, 'candidate.json');
await fs.writeFile(file, JSON.stringify(spec));

function privateAbsent(value) {
  for (const canary of canaries) assert.ok(!value.includes(canary), 'Private response canary escaped');
}
async function cli(args, env) {
  return await new Promise((resolve, reject) => {
    const child = spawn(binary, ['--format', 'json', '--allow-local', ...args], {env});
    let stdout = '', stderr = '';
    const timer = setTimeout(() => {child.kill(); reject(Error('CLI timed out'));}, 30000);
    child.stdout.on('data', data => {stdout += data;});
    child.stderr.on('data', data => {stderr += data;});
    child.once('error', error => {clearTimeout(timer); reject(error);});
    child.once('close', (code, signal) => {clearTimeout(timer); resolve({code, signal, stdout, stderr});});
  });
}
async function persistedPrivateAbsent(directory) {
  for (const item of await fs.readdir(directory, {withFileTypes: true})) {
    const file = path.join(directory, item.name);
    if (item.isDirectory()) await persistedPrivateAbsent(file);
    else if (item.isFile()) privateAbsent((await fs.readFile(file)).toString('utf8'));
  }
}
function errorContract(value, test, hasNextStep = false) {
  assert.equal(value.error?.code, test.code, test.name);
  if (hasNextStep) {
    assert.equal(typeof value.error.next_step, 'string');
    assert.ok(value.error.next_step.length > 0);
  } else assert.ok(!Object.hasOwn(value.error, 'next_step'));
  assert.deepEqual(value.error.diagnostic, {
    upstream_status: test.status, defense: test.defense,
    ...(test.reason ? {provider_reason: test.reason} : {}),
  }, 'Diagnostics contain only normalized finite labels and upstream status');
  privateAbsent(JSON.stringify(value));
  assert.ok(!Object.hasOwn(value, 'body'));
  if (test.code === 'api_denied' && hasNextStep) assert.doesNotMatch(value.error.next_step, /browser|captcha|reconnect|sign.in/i);
}
function mcp(env) {
  const child = spawn(binary, ['mcp', '--allow-local'], {env, stdio: ['pipe', 'pipe', 'pipe']});
  let buffer = '', stderr = '', sequence = 0;
  const pending = new Map();
  function fail(error) {
    for (const slot of pending.values()) {clearTimeout(slot.timer); slot.reject(error);}
    pending.clear();
  }
  child.on('error', fail);
  child.on('close', () => fail(Error('MCP exited before responding')));
  child.stderr.on('data', data => {stderr += data;});
  child.stdout.on('data', chunk => {
    buffer += chunk;
    while (buffer.includes('\n')) {
      const end = buffer.indexOf('\n'), line = buffer.slice(0, end);
      buffer = buffer.slice(end + 1);
      if (!line.trim()) continue;
      try {
        privateAbsent(line);
        const response = JSON.parse(line), slot = pending.get(response.id);
        if (!slot) continue;
        clearTimeout(slot.timer); pending.delete(response.id);
        response.error ? slot.reject(Error(JSON.stringify(response.error))) : slot.resolve(response.result);
      } catch (error) {fail(error);}
    }
  });
  return {
    rpc(method, params) {
      return new Promise((resolve, reject) => {
        const id = ++sequence;
        const timer = setTimeout(() => {pending.delete(id); reject(Error('MCP timed out'));}, 30000);
        pending.set(id, {resolve, reject, timer});
        child.stdin.write(JSON.stringify({jsonrpc: '2.0', id, method, params}) + '\n');
      });
    },
    initialized() {child.stdin.write(JSON.stringify({jsonrpc: '2.0', method: 'notifications/initialized'}) + '\n');},
    async close() {
      if (!child.pid || child.exitCode !== null || child.signalCode !== null) {privateAbsent(stderr); return;}
      const closed = new Promise(resolve => child.once('close', resolve));
      child.stdin.end(); child.kill('SIGTERM');
      await closed;
      privateAbsent(stderr);
    },
  };
}

try {
  for (const test of cases) {
    active = test;
    const definition = structuredClone(spec);
    if (test.projected) {
      definition.spec_version = 3;
      definition.models = {projection: {private: {path: 'body', type: 'string'}}};
      Object.assign(definition.operations[0], {model: 'projection',
        response: {format: 'json', required_pointers: ['/items']},
        pagination: {input: 'cursor', query: 'pageToken', next: '/nextPageToken'}});
    }
    await fs.writeFile(file, JSON.stringify(definition));
    for (const surface of ['candidate-try', 'installed-try', 'run', 'mcp-generic', 'mcp-dynamic']) {
      // Policy state is intentionally isolated per surface; 429/challenge backoff
      // must not mask a later test's actual upstream response.
      const data = path.join(root, `${test.name}-${surface}`);
      const env = {...process.env, HYCLI_DATA_DIR: data};
      if (surface !== 'candidate-try') {
        const installed = await cli(['spec', 'install', file], env);
        assert.equal(installed.code, 0, installed.stderr);
      }
      requests = [];
      const healthyAfter = test.code === 'api_denied';
      if (surface.startsWith('mcp-')) {
        const client = mcp(env);
        try {
          await client.rpc('initialize', {protocolVersion: '2025-03-26', capabilities: {},
            clientInfo: {name: 'http-denial-regression', version: '1'}});
          client.initialized();
          const call = action => client.rpc('tools/call', surface === 'mcp-generic'
            ? {name: 'hycli_run', arguments: {site, action, inputs: {}}}
            : {name: `${site}_${action}`, arguments: {}});
          const response = await call('failure');
          assert.equal(response.isError, true, `${surface}/${test.name}`);
          errorContract(JSON.parse(response.content.find(item => item.type === 'text').text), test, true);
          if (healthyAfter) {
            const healthy = await call('healthy');
            assert.ok(!healthy.isError, JSON.stringify(healthy));
            assert.equal(JSON.parse(healthy.content.find(item => item.type === 'text').text).body.ok, true);
          }
        } finally {await client.close();}
      } else {
        const args = action => surface === 'run' ? [site, action]
          : ['try', surface === 'candidate-try' ? file : site, action];
        const response = await cli(args('failure'), env);
        assert.equal(response.code, test.exit || 4, `${surface}/${test.name}: unexpected CLI exit`);
        privateAbsent(response.stdout + response.stderr);
        if (test.code === 'api_denied') assert.doesNotMatch(response.stderr, /browser|captcha|reconnect|sign.in/i);
        assert.equal(response.stdout.trim(), '', 'An HTTP failure must never be emitted as successful output');
        const lines = response.stderr.split('\n').filter(line => line.trim().startsWith('{'));
        assert.equal(lines.length, 1, 'Exactly one structured error accompanies the existing text error');
        errorContract(JSON.parse(lines[0]), test);
        if (healthyAfter) {
          const healthy = await cli(args('healthy'), env);
          assert.equal(healthy.code, 0, healthy.stderr);
          const value = JSON.parse(healthy.stdout);
          assert.equal((surface === 'run' ? value : value.body).ok, true);
        }
      }
      assert.ok(requests.includes('/failure'), 'The tested error came from the owned upstream fixture');
      if (healthyAfter) assert.ok(requests.includes('/healthy'), '403 did not poison subsequent healthy reads');
      await persistedPrivateAbsent(data);
      console.log(`HTTP denial: ${test.name} via ${surface} passed`);
    }
  }
  console.log('Actual CLI and MCP HTTP denial contracts passed with private-response canaries.');
} finally {
  await new Promise(resolve => server.close(resolve));
  await fs.rm(root, {recursive: true, force: true});
}

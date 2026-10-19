import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {fileURLToPath, pathToFileURL} from 'node:url';
import {buildServerJson} from './registry-server.mjs';

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const platforms = ['linux-x64', 'linux-arm64', 'darwin-x64', 'darwin-arm64', 'win32-x64', 'win32-arm64'];
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
async function fixture(t, selected = [], binary) {
  const scratch = path.join(root, '.cache', 'registry-tests');
  await fs.mkdir(scratch, {recursive: true});
  const dir = await fs.mkdtemp(path.join(scratch, 'case with spaces #hash-'));
  t.after(() => fs.rm(dir, {recursive: true, force: true}));
  await fs.mkdir(path.join(dir, 'scripts'));
  for (const name of ['mcpb.mjs', 'registry-server.mjs', 'publish-release.mjs']) {
    await fs.copyFile(path.join(root, 'scripts', name), path.join(dir, 'scripts', name));
  }
  await fs.writeFile(path.join(dir, 'Cargo.toml'), '[package]\nversion = "0.1.0"\n');
  await fs.mkdir(path.join(dir, 'assets'));
  await fs.writeFile(path.join(dir, 'fake-mcpb.mjs'), `
import fs from 'node:fs';
const [verb,stage,output] = process.argv.slice(2);
if (verb === 'validate') JSON.parse(fs.readFileSync(stage));
else if (verb === 'pack') fs.writeFileSync(output, fs.readFileSync(stage + '/manifest.json'));
else throw new Error('Unexpected test CLI command');
`);
  const source = path.join(dir, 'input', 'package');
  for (const name of ['agent', 'skills/hycli']) await fs.mkdir(path.join(source, name), {recursive: true});
  for (const name of ['hycli', 'hycli.exe', 'LICENSE', 'README.md', 'agent/AGENTS.md', 'skills/hycli/SKILL.md']) {
    await fs.writeFile(path.join(source, name), name === 'hycli' && binary ? binary : `synthetic ${name}\n`);
  }
  const env = {...process.env};
  delete env.TAR_OPTIONS;
  // Stream the output to avoid Windows drive-letter interpretation in GNU tar.
  const packed = spawnSync('tar', ['-czf', '-', 'package'], {cwd: path.dirname(source), env, maxBuffer: 64 * 1024 * 1024});
  assert.equal(packed.status, 0, String(packed.stderr));
  for (const platform of selected) {
    await fs.writeFile(path.join(dir, 'assets', `hycli-0.1.0-${platform}.tar.gz`), packed.stdout);
  }
  return {dir, archive: packed.stdout};
}
function run(dir, script, args = [], preload) {
  return spawnSync(process.execPath, [...(preload ? ['--import', pathToFileURL(path.join(dir, preload)).href] : []),
    path.join(dir, 'scripts', script), ...args], {cwd: dir, encoding: 'utf8', timeout: 30000});
}
function bundleArgs(dir) {
  return ['--assets', 'assets', '--output', 'output', '--mcpb', path.join(dir, 'fake-mcpb.mjs'), '--server-json', 'server.json'];
}
async function fetchMock(dir, archive, corrupt = false) {
  await fs.writeFile(path.join(dir, 'remote.tar.gz'), archive);
  await fs.writeFile(path.join(dir, 'fetch-mock.mjs'), `
import fs from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const dir = path.dirname(fileURLToPath(import.meta.url));
const bytes = await fs.readFile(path.join(dir, 'remote.tar.gz'));
globalThis.fetch = async url => {
  if (!/^https:\\/\\/github\\.com\\/Hybirdss\\/Hycli\\/releases\\/download\\/v0\\.1\\.0\\/hycli-0\\.1\\.0-(linux|darwin|win32)-(x64|arm64)\\.tar\\.gz(\\.sha256)?$/.test(url)) throw new Error('Unexpected fetch: ' + url);
  return new Response(url.endsWith('.sha256') ? '${corrupt ? '0'.repeat(64) : digest(archive)}  archive\\n' : bytes);
};
`);
}

test('local default discovers only existing native archives and hashes output', async t => {
  const {dir} = await fixture(t, ['darwin-arm64']);
  const result = run(dir, 'mcpb.mjs', bundleArgs(dir));
  assert.equal(result.status, 0, result.stderr);
  const server = JSON.parse(await fs.readFile(path.join(dir, 'server.json')));
  assert.equal(server.packages.length, 1);
  assert.equal(server.websiteUrl, 'https://github.com/Hybirdss/Hycli#use-hycli-with-your-ai');
  const file = 'hycli-mcp-0.1.0-darwin-arm64.mcpb';
  assert.ok(server.packages[0].identifier.endsWith('/' + file));
  assert.equal(server.packages[0].fileSha256, digest(await fs.readFile(path.join(dir, 'output', file))));
});

test('large gzip archive preserves the complete extracted executable', async t => {
  const tarVersion = spawnSync('tar', ['--version'], {encoding: 'utf8'});
  assert.equal(tarVersion.status, 0, tarVersion.stderr);
  t.diagnostic(tarVersion.stdout.split(/\r?\n/)[0]);
  // Deterministic, incompressible payload comparable to a real release archive.
  // Tiny fixtures cannot exercise the Windows buffered-stdin failure boundary.
  const binary = Buffer.alloc(16 * 1024 * 1024);
  let state = 0x12345678;
  for (let i = 0; i < binary.length; i++) {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    binary[i] = state & 255;
  }
  const {dir, archive} = await fixture(t, ['linux-x64'], binary);
  assert.ok(archive.length > 15 * 1024 * 1024, 'fixture must remain a large compressed input');
  const result = run(dir, 'mcpb.mjs', [...bundleArgs(dir), '--keep-stage']);
  assert.equal(result.status, 0, result.stderr);
  const extracted = await fs.readFile(path.join(dir, 'dist/mcpb/stage/hycli-mcp-0.1.0-linux-x64/server/hycli'));
  assert.equal(extracted.length, binary.length);
  assert.equal(digest(extracted), digest(binary));
});

test('invalid gzip input fails extraction instead of packing partial files', async t => {
  const {dir} = await fixture(t, ['linux-x64']);
  await fs.writeFile(path.join(dir, 'assets/hycli-0.1.0-linux-x64.tar.gz'), 'not a gzip archive');
  const result = run(dir, 'mcpb.mjs', bundleArgs(dir));
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /tar .*failed/);
  assert.deepEqual(await fs.readdir(path.join(dir, 'output')), []);
});

test('empty local default and unknown platform fail clearly', async t => {
  const {dir} = await fixture(t);
  const empty = run(dir, 'mcpb.mjs', bundleArgs(dir));
  assert.notEqual(empty.status, 0);
  assert.match(empty.stderr, /No native archives found/);
  const unknown = run(dir, 'mcpb.mjs', [...bundleArgs(dir), '--platform', 'unknown']);
  assert.notEqual(unknown.status, 0);
  assert.match(unknown.stderr, /Unknown platform/);
});

test('explicit missing platform fails before producing a partial set', async t => {
  const {dir} = await fixture(t, ['linux-x64']);
  const result = run(dir, 'mcpb.mjs', [...bundleArgs(dir), '--platform', 'linux-x64,linux-arm64']);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Missing .*linux-arm64/);
  await assert.rejects(fs.access(path.join(dir, 'output')));
});

test('fetch defaults to all six platforms and verifies archive checksums', async t => {
  const {dir, archive} = await fixture(t);
  await fetchMock(dir, archive);
  const result = run(dir, 'mcpb.mjs', [...bundleArgs(dir), '--fetch'], 'fetch-mock.mjs');
  assert.equal(result.status, 0, result.stderr);
  const server = JSON.parse(await fs.readFile(path.join(dir, 'server.json')));
  assert.equal(server.packages.length, 6);
  for (const platform of platforms) {
    const file = `hycli-mcp-0.1.0-${platform}.mcpb`;
    const bytes = await fs.readFile(path.join(dir, 'output', file));
    assert.equal(server.packages.find(item => item.identifier.endsWith('/' + file)).fileSha256, digest(bytes));
  }
});

test('fetch rejects a mismatched release checksum before packing', async t => {
  const {dir, archive} = await fixture(t);
  await fetchMock(dir, archive, true);
  const result = run(dir, 'mcpb.mjs', [...bundleArgs(dir), '--fetch', '--platform', 'linux-x64'], 'fetch-mock.mjs');
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Release checksum mismatch/);
  assert.deepEqual(await fs.readdir(path.join(dir, 'output')), []);
});

async function publisher(t, {tag = 'v0.1.0', missing, corrupt} = {}) {
  const {dir} = await fixture(t);
  const source = path.join(dir, 'artifacts');
  for (const platform of platforms) {
    const folder = path.join(source, 'hycli-' + platform);
    await fs.mkdir(folder, {recursive: true});
    if (platform === missing) continue;
    const name = `hycli-mcp-0.1.0-${platform}.mcpb`, bytes = Buffer.from(platform);
    await fs.writeFile(path.join(folder, name), platform === corrupt ? 'corrupt' : bytes);
    await fs.writeFile(path.join(folder, name + '.sha256'), `${digest(bytes)}  ${name}\n`);
  }
  // Intercept the subprocess at its boundary, portable across Windows/macOS/Linux.
  // Fail closed: no real gh command (especially upload) can run in these tests.
  await fs.writeFile(path.join(dir, 'gh-mock.mjs'), `
import cp from 'node:child_process';
import fs from 'node:fs';
import fsp from 'node:fs/promises';
import path from 'node:path';
import {syncBuiltinESMExports} from 'node:module';
import {fileURLToPath} from 'node:url';
const dir = path.dirname(fileURLToPath(import.meta.url));
cp.spawnSync = (command, args) => {
  if (command !== 'gh' || args[0] !== 'run' || args[1] !== 'download') throw new Error('Forbidden subprocess: ' + command + ' ' + args);
  fs.cpSync(path.join(dir, 'artifacts'), args[args.indexOf('--dir') + 1], {recursive: true});
  return {status: 0, stdout: '', stderr: ''};
};
syncBuiltinESMExports();
const write = fsp.writeFile;
fsp.writeFile = async (file, bytes, ...options) => {
  if (path.basename(file) === 'server.json') await write(path.join(dir, 'saved-server.json'), bytes);
  return write(file, bytes, ...options);
};
`);
  const result = run(dir, 'publish-release.mjs', ['--dry-run', '--run-id', '123', '--tag', tag], 'gh-mock.mjs');
  return {dir, result};
}

test('publisher uses the actual release tag for every download URL', async t => {
  for (const tag of ['v0.1.0', 'testing-v0.1.0', 'release/0.1.0']) {
    const {dir, result} = await publisher(t, {tag});
    assert.equal(result.status, 0, result.stderr);
    const server = JSON.parse(await fs.readFile(path.join(dir, 'saved-server.json')));
    assert.equal(server.packages.length, 6);
    for (const item of server.packages) assert.ok(item.identifier.includes(`/download/${encodeURIComponent(tag)}/`));
  }
});

test('publisher refuses a missing platform', async t => {
  const {result} = await publisher(t, {missing: 'win32-arm64'});
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Missing MCP bundles: win32-arm64/);
});

test('publisher refuses corrupted bundle bytes', async t => {
  const {result} = await publisher(t, {corrupt: 'linux-x64'});
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /SHA-256 mismatch/);
});

test('registry metadata rejects malformed hashes and empty bundle sets', () => {
  assert.throws(() => buildServerJson({version: '0.1.0', bundles: []}), /No MCP bundles/);
  assert.throws(() => buildServerJson({version: '0.1.0', bundles: [{file: 'hycli.mcpb', sha256: 'deadbeef'}]}), /Invalid sha256/);
});

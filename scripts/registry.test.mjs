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

async function publisher(t, {tag = 'v0.1.0', missing, corrupt, mutate, runInfo = {}, tagVersion = '0.1.0', automatic = false, noRuns = false, dryRun = true} = {}) {
  const {dir} = await fixture(t);
  const source = path.join(dir, 'artifacts');
  for (const platform of platforms) {
    const folder = path.join(source, 'hycli-' + platform);
    await fs.mkdir(folder, {recursive: true});
    if (platform === missing) continue;
    const [os, arch] = platform.split('-'), stem = `hycli-0.1.0-${platform}`;
    const bundle = `hycli-mcp-0.1.0-${platform}.mcpb`;
    const installer = stem + (os === 'linux' ? '.deb' : os === 'darwin' ? '.dmg' : '-setup.exe');
    for (const name of [stem + '.tar.gz', bundle, installer, ...(platform === 'linux-x64' ? ['hycli-0.1.0-source.tar.gz'] : [])]) {
      const bytes = Buffer.from('synthetic ' + name);
      await fs.writeFile(path.join(folder, name), name === bundle && platform === corrupt ? 'corrupt' : bytes);
      await fs.writeFile(path.join(folder, name + '.sha256'), `${digest(bytes)}  ${name}\n`);
      if (name.endsWith('.tar.gz')) {
        const isSource = name.includes('-source.');
        await fs.writeFile(path.join(folder, name.replace('.tar.gz', '.manifest.json')), JSON.stringify({
          version: '0.1.0', kind: isSource ? 'source' : 'native', platform: isSource ? null : os,
          arch: isSource ? null : arch, files: ['synthetic-file', 'SHA256SUMS'], sha256: digest(bytes)
        }));
      }
    }
  }
  if (mutate) await mutate(source);
  const metadata = {id: 123, status: 'completed', conclusion: 'success', head_branch: 'main',
    head_sha: 'a'.repeat(40), event: 'push', path: '.github/workflows/verify.yml', ...runInfo};
  // Intercept all subprocesses before publisher imports. Nothing falls through to real gh.
  await fs.writeFile(path.join(dir, 'gh-mock.mjs'), `
import cp from 'node:child_process';
import fs from 'node:fs';
import fsp from 'node:fs/promises';
import path from 'node:path';
import {syncBuiltinESMExports} from 'node:module';
import {fileURLToPath} from 'node:url';
const dir = path.dirname(fileURLToPath(import.meta.url));
const calls = [];
cp.spawnSync = (command, args) => {
  calls.push([command, ...args]);
  fs.writeFileSync(path.join(dir, 'gh-calls.json'), JSON.stringify(calls));
  if (command !== 'gh') throw new Error('Forbidden subprocess: ' + command);
  let value;
  if (args[0] === 'run' && args[1] === 'download') {
    fs.cpSync(path.join(dir, 'artifacts'), args[args.indexOf('--dir') + 1], {recursive: true});
    return {status: 0, stdout: '', stderr: ''};
  } else if (args[0] === 'run' && args[1] === 'list') value = ${JSON.stringify(noRuns ? [] : [{databaseId: 123}])};
  else if (args[0] === 'api' && args[1] === 'repos/{owner}/{repo}/actions/runs/123') value = ${JSON.stringify(metadata)};
  else if (args[0] === 'api' && args[1] === ${JSON.stringify('repos/{owner}/{repo}/commits/' + encodeURIComponent('refs/tags/' + tag))}) value = {sha: '${'a'.repeat(40)}'};
  else if (args[0] === 'api' && args[1] === 'repos/{owner}/{repo}/contents/Cargo.toml?ref=${'a'.repeat(40)}') value = {encoding: 'base64', content: ${JSON.stringify(Buffer.from('[package]\nversion = "' + tagVersion + '"\n').toString('base64'))}};
  else throw new Error('Forbidden subprocess: ' + command + ' ' + args);
  return {status: 0, stdout: JSON.stringify(value), stderr: ''};
};
syncBuiltinESMExports();
globalThis.fetch = () => { throw new Error('Network forbidden'); };
const write = fsp.writeFile;
fsp.writeFile = async (file, bytes, ...options) => {
  if (['server.json', 'SHA256SUMS'].includes(path.basename(file))) await write(path.join(dir, 'saved-' + path.basename(file)), bytes);
  return write(file, bytes, ...options);
};
`);
  const args = [...(dryRun ? ['--dry-run'] : []), ...(automatic ? [] : ['--run-id', '123']), '--tag', tag];
  const result = run(dir, 'publish-release.mjs', args, 'gh-mock.mjs');
  const calls = JSON.parse(await fs.readFile(path.join(dir, 'gh-calls.json'), 'utf8').catch(() => '[]'));
  assert.ok(!calls.some(call => call[1] === 'release'), 'publisher must not attempt upload in this fixture');
  return {dir, result, calls};
}

test('publisher validates complete artifacts and preserves custom release tags', async t => {
  for (const tag of ['v0.1.0', 'testing-v0.1.0', 'release/0.1.0']) {
    const {dir, result} = await publisher(t, {tag});
    assert.equal(result.status, 0, result.stderr);
    const server = JSON.parse(await fs.readFile(path.join(dir, 'saved-server.json')));
    assert.equal(server.packages.length, 6);
    for (const item of server.packages) assert.ok(item.identifier.includes(`/download/${encodeURIComponent(tag)}/`));
    const summary = JSON.parse(result.stdout.trim().split('\n').at(-1));
    assert.equal(summary.total_files, 47);
    assert.equal(summary.verified_artifacts, 19);
    assert.equal((await fs.readFile(path.join(dir, 'saved-SHA256SUMS'), 'utf8')).trim().split('\n').length, 19);
  }
});

test('publisher automatic selection receives the same run validation', async t => {
  const valid = await publisher(t, {automatic: true});
  assert.equal(valid.result.status, 0, valid.result.stderr);
  assert.ok(valid.calls.some(call => call.includes('repos/{owner}/{repo}/actions/runs/123')));
  const failed = await publisher(t, {automatic: true, runInfo: {conclusion: 'failure'}});
  assert.notEqual(failed.result.status, 0);
  assert.ok(!failed.calls.some(call => call[2] === 'download'));
});

for (const [name, runInfo] of [
  ['failed run', {conclusion: 'failure'}], ['unfinished run', {status: 'in_progress'}],
  ['wrong workflow', {path: '.github/workflows/other.yml'}], ['wrong branch', {head_branch: 'feature'}],
  ['pull request run', {event: 'pull_request'}], ['different source', {head_sha: 'b'.repeat(40)}]
]) test(`publisher refuses explicit ${name} before downloading`, async t => {
  const {result, calls} = await publisher(t, {runInfo});
  assert.notEqual(result.status, 0, 'invalid run must not pass collection');
  assert.ok(!calls.some(call => call[2] === 'download'), 'invalid run must be rejected before download');
});

test('publisher refuses a tag whose source version differs from Cargo', async t => {
  const {result, calls} = await publisher(t, {tagVersion: '0.2.0'});
  assert.notEqual(result.status, 0, 'tag version mismatch must not pass');
  assert.match(result.stderr, /version/i);
  assert.ok(!calls.some(call => call[2] === 'download'));
});

test('publisher refuses an empty successful-run selection', async t => {
  const {result} = await publisher(t, {automatic: true, noRuns: true});
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /No successful/);
});

test('publisher refuses a missing platform', async t => {
  const {result} = await publisher(t, {missing: 'win32-arm64'});
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Missing .*artifact/i);
});

test('publisher refuses corrupted bundle bytes', async t => {
  const {result} = await publisher(t, {corrupt: 'linux-x64'});
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /SHA-256 mismatch/);
});

for (const file of ['hycli-0.1.0-linux-x64.tar.gz', 'hycli-0.1.0-linux-x64.deb', 'hycli-mcp-0.1.0-linux-x64.mcpb',
  'hycli-0.1.0-source.tar.gz', 'hycli-0.1.0-linux-x64.manifest.json', 'hycli-0.1.0-source.manifest.json',
  'hycli-0.1.0-linux-x64.tar.gz.sha256', 'hycli-0.1.0-linux-x64.deb.sha256',
  'hycli-mcp-0.1.0-linux-x64.mcpb.sha256', 'hycli-0.1.0-source.tar.gz.sha256']) {
  test(`publisher refuses missing ${file}`, async t => {
    const {result} = await publisher(t, {mutate: source => fs.rm(path.join(source, 'hycli-linux-x64', file))});
    assert.notEqual(result.status, 0, 'incomplete artifact set must not pass');
    assert.match(result.stderr, /Missing .*artifact/i);
  });
}

for (const payload of ['hycli-0.1.0-linux-x64.tar.gz', 'hycli-0.1.0-linux-x64.deb', 'hycli-0.1.0-source.tar.gz']) {
  test(`publisher refuses absent payload and sidecar together for ${payload}`, async t => {
    const {result} = await publisher(t, {mutate: async source => {
      for (const name of [payload, payload + '.sha256']) await fs.rm(path.join(source, 'hycli-linux-x64', name));
    }});
    assert.notEqual(result.status, 0, 'missing artifact class must not pass');
    assert.match(result.stderr, /Missing .*artifact/i);
  });
}

test('publisher refuses duplicate basenames instead of overwriting', async t => {
  const {result} = await publisher(t, {mutate: source => fs.copyFile(
    path.join(source, 'hycli-linux-x64', 'hycli-mcp-0.1.0-linux-x64.mcpb'),
    path.join(source, 'hycli-win32-arm64', 'hycli-mcp-0.1.0-linux-x64.mcpb'))});
  assert.notEqual(result.status, 0, 'duplicate basename must not pass');
  assert.match(result.stderr, /Duplicate/);
});

for (const [name, mutate] of [
  ['wrong sidecar filename', source => fs.writeFile(path.join(source, 'hycli-linux-x64', 'hycli-0.1.0-linux-x64.deb.sha256'), `${digest(Buffer.from('synthetic hycli-0.1.0-linux-x64.deb'))}  different.deb\n`)],
  ['unexpected artifact', source => fs.writeFile(path.join(source, 'hycli-linux-x64', 'debug-output.txt'), 'synthetic')],
  ...Object.entries({version: '0.2.0', platform: 'win32', arch: 'arm64', kind: 'source', sha256: '0'.repeat(64)}).map(([key, value]) => [
    'manifest ' + key, async source => {
      const file = path.join(source, 'hycli-linux-x64', 'hycli-0.1.0-linux-x64.manifest.json');
      const manifest = JSON.parse(await fs.readFile(file)); manifest[key] = value; await fs.writeFile(file, JSON.stringify(manifest));
    }
  ])
]) test(`publisher refuses ${name}`, async t => {
  const {result} = await publisher(t, {mutate});
  assert.notEqual(result.status, 0, name + ' must not pass');
});

test('publisher validation failure cannot reach upload without dry-run', async t => {
  const {result} = await publisher(t, {dryRun: false, runInfo: {conclusion: 'failure'}});
  assert.notEqual(result.status, 0);
  assert.doesNotMatch(result.stderr, /Forbidden subprocess/);
});

test('registry metadata rejects malformed hashes and empty bundle sets', () => {
  assert.throws(() => buildServerJson({version: '0.1.0', bundles: []}), /No MCP bundles/);
  assert.throws(() => buildServerJson({version: '0.1.0', bundles: [{file: 'hycli.mcpb', sha256: 'deadbeef'}]}), /Invalid sha256/);
});

const nativeAssets = {
  linux: {'install.sh': 'packaging/install-linux.sh', 'hycli.png': 'packaging/icons/hycli.png'},
  darwin: {'install.sh': 'packaging/install-macos.sh', 'hycli.png': 'packaging/icons/hycli.png'},
  win32: {'install.ps1': 'packaging/install-windows.ps1', 'hycli.ico': 'packaging/icons/hycli.ico'},
};

test('native archives contain only public files with platform installers, exact bytes and hashes', async t => {
  for (const target of platforms) await t.test(target, async t => {
    const [platform, arch] = target.split('-');
    const executable = platform === 'win32' ? 'hycli.exe' : 'hycli';
    const launcher = platform === 'win32' ? 'hycli-desktop.exe' : 'hycli-desktop';
    const {dir} = await fixture(t);
    await fs.copyFile(path.join(root, 'scripts/package.mjs'), path.join(dir, 'scripts/package.mjs'));
    const publicFiles = ['LICENSE', 'agent/AGENTS.md', 'agent/CLAUDE.md', 'docs/SITESPEC.md',
      'docs/ACCOUNT_READ.md', 'docs/BUILD.md', 'docs/RELEASING.md', 'skills/hycli/SKILL.md'];
    const sources = {...Object.fromEntries(publicFiles.map(file => [file, file])),
      [executable]: 'dist/' + executable, [launcher]: 'dist/' + launcher,
      'README.md': 'dist/README.md', 'build.json': 'dist/build.json', ...nativeAssets[platform]};
    const files = [...new Set([...Object.values(sources).filter(file => file !== 'dist/build.json'),
      ...Object.values(nativeAssets).flatMap(assets => Object.values(assets)),
      'dist/private-session.json', 'packaging/private-session.json', 'packaging/icons/unrelated.png'])];
    for (const file of files) {
      await fs.mkdir(path.dirname(path.join(dir, file)), {recursive: true});
      await fs.writeFile(path.join(dir, file), `synthetic ${file}\n`);
    }
    const binaries = {};
    for (const name of [executable, launcher]) binaries[name] = digest(await fs.readFile(path.join(dir, 'dist', name)));
    await fs.writeFile(path.join(dir, 'dist/build.json'), JSON.stringify({platform, arch, sha256: binaries[executable], binaries}));
    await fs.writeFile(path.join(dir, 'assert-stream.mjs'), `
import assert from 'node:assert/strict';
import cp from 'node:child_process';
import {syncBuiltinESMExports} from 'node:module';
const original = cp.spawnSync;
cp.spawnSync = (command, args, options) => {
  if (command === 'tar' && args.includes('-czf')) {
    assert.equal(args[args.indexOf('-czf') + 1], '-', 'Do not expose D:\\\\archive paths to tar remote-host parsing');
    assert.ok(!args.includes('-C'), 'Node cwd must handle native drive-letter directories');
    assert.equal(typeof options.stdio[1], 'number', 'Archive must stream to an already opened file');
    assert.ok(options.cwd);
  }
  return original(command, args, options);
};
syncBuiltinESMExports();
`);
    const result = run(dir, 'package.mjs', [], 'assert-stream.mjs');
    assert.equal(result.status, 0, result.stderr);
    const output = JSON.parse(result.stdout.trim());
    const archive = await fs.readFile(output.archive);
    assert.equal(output.sha256, digest(archive));
    assert.equal(await fs.readFile(output.archive + '.sha256', 'utf8'), `${digest(archive)}  ${path.basename(output.archive)}\n`);
    const manifest = JSON.parse(await fs.readFile(output.archive.replace(/\.tar\.gz$/, '.manifest.json')));
    assert.equal(manifest.platform, platform);
    assert.equal(manifest.arch, arch);
    assert.equal(manifest.sha256, digest(archive));
    assert.deepEqual([...manifest.files].sort(), [...Object.keys(sources), 'SHA256SUMS'].sort());
    const unpacked = path.join(dir, 'verify archive');
    await fs.mkdir(unpacked);
    const archiveHandle = await fs.open(output.archive, 'r');
    let extraction;
    try { extraction = spawnSync('tar', ['-xzf', '-'], {cwd: unpacked, stdio: [archiveHandle.fd, 'pipe', 'pipe']}); }
    finally { await archiveHandle.close(); }
    assert.equal(extraction.status, 0, String(extraction.stderr));
    const content = path.join(unpacked, `hycli-0.1.0-${target}`);
    const entries = await fs.readdir(content, {recursive: true});
    const archivedFiles = [];
    for (const entry of entries) if ((await fs.stat(path.join(content, entry))).isFile()) archivedFiles.push(entry.split(path.sep).join('/'));
    assert.deepEqual(archivedFiles.sort(), [...manifest.files].sort());
    for (const [file, source] of Object.entries(sources)) {
      assert.deepEqual(await fs.readFile(path.join(content, file)), await fs.readFile(path.join(dir, source)), file);
    }
    if (process.platform !== 'win32' && platform !== 'win32') {
      assert.equal((await fs.stat(path.join(content, 'install.sh'))).mode & 0o777, 0o755);
    }
    const sums = (await fs.readFile(path.join(content, 'SHA256SUMS'), 'utf8')).trim().split('\n');
    assert.deepEqual(sums.map(line => line.slice(66)).sort(), manifest.files.filter(file => file !== 'SHA256SUMS').sort());
    for (const line of sums) assert.equal(digest(await fs.readFile(path.join(content, line.slice(66)))), line.slice(0, 64));
    for (const [name, hash] of Object.entries(binaries)) assert.equal(digest(await fs.readFile(path.join(content, name))), hash);
  });
});

test('native smoke check requires checksummed platform assets before executing binaries', async t => {
  for (const target of platforms) await t.test(target, async t => {
    const [platform, arch] = target.split('-');
    const {dir} = await fixture(t);
    await fs.copyFile(path.join(root, 'scripts/smoke-package.mjs'), path.join(dir, 'scripts/smoke-package.mjs'));
    await fs.writeFile(path.join(dir, 'smoke-preflight.mjs'), `
import cp from 'node:child_process';
import path from 'node:path';
import {syncBuiltinESMExports} from 'node:module';
Object.defineProperty(process, 'platform', {value: '${platform}'});
Object.defineProperty(process, 'arch', {value: '${arch}'});
const original = cp.spawnSync;
cp.spawnSync = (command, args, options) => {
  if (command === 'tar') return original(command, args, options);
  if (path.basename(command) === '${platform === 'win32' ? 'hycli.exe' : 'hycli'}') throw Error('Reached native execution boundary');
  throw Error('Unexpected subprocess: ' + command);
};
cp.spawn = command => {throw Error('Unexpected subprocess: ' + command);};
syncBuiltinESMExports();
`);
    const source = path.join(dir, 'input/package');
    const assets = Object.keys(nativeAssets[platform]);
    const cases = [{name: 'complete'}];
    for (const asset of assets) {
      cases.push({name: 'missing ' + asset, omitted: asset, missing: true});
      cases.push({name: 'unchecksummed ' + asset, omitted: asset});
    }
    for (const scenario of cases) await t.test(scenario.name, async () => {
      for (const asset of assets) await fs.writeFile(path.join(source, asset), 'synthetic ' + asset);
      if (scenario.missing) await fs.rm(path.join(source, scenario.omitted));
      const sums = [];
      for (const file of ['hycli', 'hycli.exe', ...assets.filter(asset => asset !== scenario.omitted)]) {
        sums.push(`${digest(await fs.readFile(path.join(source, file)))}  ${file}`);
      }
      await fs.writeFile(path.join(source, 'SHA256SUMS'), sums.join('\n') + '\n');
      const env = {...process.env};
      delete env.TAR_OPTIONS;
      const packed = spawnSync('tar', ['-czf', '-', 'package'], {cwd: path.dirname(source), env});
      assert.equal(packed.status, 0, String(packed.stderr));
      const archive = path.join(dir, 'smoke.tar.gz');
      await fs.writeFile(archive, packed.stdout);
      const result = run(dir, 'smoke-package.mjs', [archive], 'smoke-preflight.mjs');
      assert.notEqual(result.status, 0);
      if (scenario.omitted) {
        assert.match(result.stderr, /Required native package asset missing from SHA256SUMS/);
        assert.ok(result.stderr.includes(scenario.omitted), result.stderr);
        assert.doesNotMatch(result.stderr, /Reached native execution boundary/);
      } else {
        assert.match(result.stderr, /Reached native execution boundary/);
      }
    });
  });
});

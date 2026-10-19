#!/usr/bin/env node
// Build MCP bundles (.mcpb) from an already published release archive and, optionally, write the
// registry `server.json` whose `packages[].fileSha256` values come from those bundles.
//
// Input is the packaged archive produced by scripts/package.mjs (`dist/artifacts/hycli-<version>-<platform>.tar.gz`),
// never a fresh source build: the registry entry must point at the exact bytes users download.
//
// Usage:
//   node scripts/mcpb.mjs [--assets <dir>] [--output <dir>] [--platform linux-x64] [--server-json <path>]
//                         [--mcpb <path>] [--keep-stage]
import fs from 'node:fs/promises';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {parseArgs} from 'node:util';
import {buildServerJson, readVersion} from './registry-server.mjs';

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const {values} = parseArgs({
  options: {
    assets: {type: 'string'},
    output: {type: 'string'},
    platform: {type: 'string', multiple: true},
    'server-json': {type: 'string'},
    mcpb: {type: 'string'},
    fetch: {type: 'boolean'},
    'keep-stage': {type: 'boolean'},
    help: {type: 'boolean'}
  }
});

if (values.help) {
  console.log(
    'Usage: node scripts/mcpb.mjs [--assets <dir>] [--output <dir>] [--platform <os>-<arch>]... [--server-json <path>]\n' +
      '  --assets      release archives (default: dist/artifacts)\n' +
      '  --output      where .mcpb files and .sha256 sidecars are written (default: dist/artifacts)\n' +
      '  --platform    restrict to one or more platforms; default is available local archives (all platforms with --fetch)\n' +
      '  --server-json write a registry server.json with the measured fileSha256 values\n' +
      '  --fetch       download the release archives for the Cargo.toml version instead of requiring a local build\n' +
      '  --mcpb        mcpb CLI entry (default: $MCPB_CLI, then .cache/mcpb, then `mcpb` on PATH)\n' +
      '  --keep-stage  keep dist/mcpb/stage for inspection'
  );
  process.exit(0);
}

const mcpb = await resolveMcpb(values.mcpb);
const assetsDir = path.resolve(root, values.assets || path.join('dist', 'artifacts'));
const outputDir = path.resolve(root, values.output || path.join('dist', 'artifacts'));
const stageRoot = path.join(root, 'dist', 'mcpb', 'stage');
const reportPath = path.join(root, 'dist', 'mcpb', 'mcpb-report.json');

// Every platform the release workflow builds. `exe` is the binary the MCP client launches.
const PLATFORMS = [
  {platform: 'linux', arch: 'x64', exe: 'hycli'},
  {platform: 'linux', arch: 'arm64', exe: 'hycli'},
  {platform: 'darwin', arch: 'x64', exe: 'hycli'},
  {platform: 'darwin', arch: 'arm64', exe: 'hycli'},
  {platform: 'win32', arch: 'x64', exe: 'hycli.exe'},
  {platform: 'win32', arch: 'arm64', exe: 'hycli.exe'}
].map(entry => ({...entry, id: `${entry.platform}-${entry.arch}`}));

const version = await readVersion(root);
const wanted = values.platform?.length ? values.platform.flatMap(v => v.split(',')).filter(Boolean) : null;
const available = wanted || values.fetch ? null : await fs.readdir(assetsDir).catch(error => {
  if (error.code === 'ENOENT') return [];
  throw error;
});
const selected = PLATFORMS.filter(p => wanted
  ? wanted.includes(p.id)
  : values.fetch || available.includes(`hycli-${version}-${p.id}.tar.gz`));
const unknown = wanted?.filter(id => !PLATFORMS.some(p => p.id === id)) ?? [];
if (unknown.length) throw new Error(`Unknown platform(s): ${unknown.join(', ')}`);
if (!selected.length) throw new Error('No native archives found; run scripts/package.mjs or use --fetch');
// Fail before producing partial output when an explicitly requested archive is absent.
if (!values.fetch) {
  for (const entry of selected) await require(path.join(assetsDir, `hycli-${version}-${entry.id}.tar.gz`));
}

await fs.mkdir(outputDir, {recursive: true});
await fs.mkdir(path.dirname(reportPath), {recursive: true});

const report = [];
for (const entry of selected) {
  const archive = path.join(assetsDir, `hycli-${version}-${entry.id}.tar.gz`);
  if (values.fetch) await fetchReleaseArchive(entry, archive);
  await require(archive);
  const bundle = path.join(outputDir, `hycli-mcp-${version}-${entry.id}.mcpb`);
  const stage = path.join(stageRoot, `hycli-mcp-${version}-${entry.id}`);
  const unpacked = path.join(stageRoot, `${entry.id}-unpacked`);
  await fs.rm(stage, {recursive: true, force: true});
  await fs.rm(unpacked, {recursive: true, force: true});
  await fs.mkdir(path.join(stage, 'server'), {recursive: true});
  await fs.mkdir(unpacked, {recursive: true});

  // A file-backed stdin avoids both Windows drive-letter parsing and spawnSync's
  // buffered pipe writes, which can fail with EOF on large Windows archives.
  const tarEnv = {...process.env};
  delete tarEnv.TAR_OPTIONS;
  const archiveHandle = await fs.open(archive, 'r');
  try {
    // Let Node set the native cwd; Git Bash tar cannot resolve Windows drive paths in -C.
    run('tar', ['-xzf', '-', '--strip-components=1'], {
      cwd: unpacked, stdio: [archiveHandle.fd, 'pipe', 'pipe'], env: tarEnv
    });
  } finally {
    await archiveHandle.close();
  }
  await rejectLinks(unpacked);
  const binary = path.join(unpacked, entry.exe);
  await require(binary);
  await fs.copyFile(binary, path.join(stage, 'server', entry.exe));
  await fs.chmod(path.join(stage, 'server', entry.exe), 0o755);
  for (const name of ['LICENSE', 'README.md']) {
    await fs.copyFile(path.join(unpacked, name), path.join(stage, name));
  }
  // The portable skill and end-user agent guidance ship with the bundle so the MCP client's user
  // can read how the tools behave without visiting the repository.
  for (const dir of ['agent', 'skills']) {
    await fs.cp(path.join(unpacked, dir), path.join(stage, dir), {recursive: true});
  }

  const manifest = buildManifest(entry, version);
  await fs.writeFile(path.join(stage, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');

  const validated = run(mcpb.command, [...mcpb.args, 'validate', 'manifest.json'], {cwd: stage});
  console.log(`[validate] ${entry.id}: ${validated}`);

  run(mcpb.command, [...mcpb.args, 'pack', stage, bundle]);
  const bytes = await fs.readFile(bundle);
  const sha256 = createHash('sha256').update(bytes).digest('hex');
  await fs.writeFile(bundle + '.sha256', `${sha256}  ${path.basename(bundle)}\n`);
  report.push({
    platform: entry.platform,
    arch: entry.arch,
    file: path.basename(bundle),
    size: bytes.length,
    sha256,
    entry_point: manifest.server.entry_point
  });
  console.log(JSON.stringify({bundle: path.relative(root, bundle), size: bytes.length, sha256}));
  await fs.rm(unpacked, {recursive: true, force: true});
  if (!values['keep-stage']) await fs.rm(stage, {recursive: true, force: true});
}
if (!values['keep-stage']) await fs.rm(stageRoot, {recursive: true, force: true});

// Machine paths must not leak into generated metadata: an --assets directory outside the repository
// is recorded by name only.
const assetLabel = (() => {
  const relative = path.relative(root, assetsDir);
  return relative.startsWith('..') ? `external:${path.basename(assetsDir)}` : relative;
})();
await fs.writeFile(
  reportPath,
  JSON.stringify({version, generated_from: assetLabel, bundles: report}, null, 2) + '\n'
);

if (values['server-json']) {
  const target = path.resolve(root, values['server-json']);
  await fs.mkdir(path.dirname(target), {recursive: true});
  const server = buildServerJson({version, bundles: report});
  await fs.writeFile(target, JSON.stringify(server, null, 2) + '\n');
  console.log(JSON.stringify({server_json: path.relative(root, target), packages: server.packages.length}));
}

function buildManifest(entry, version) {
  // The bundle holds one platform's executable, so `mcp_config.command` names that file directly.
  // win32 additionally repeats the `.exe` spelling in platform_overrides: a client that prefers an
  // override over the base command then launches the same binary instead of falling back to `hycli`.
  const command = `\${__dirname}/server/${entry.exe}`;
  const mcpConfig = {command, args: ['mcp']};
  if (entry.platform === 'win32') mcpConfig.platform_overrides = {win32: {command, args: ['mcp']}};
  return {
    $schema: 'https://raw.githubusercontent.com/anthropics/mcpb/main/schemas/mcpb-manifest-v0.4.schema.json',
    manifest_version: '0.4',
    name: 'hycli',
    display_name: 'Hycli',
    version,
    description: 'Turn any website into a typed CLI and MCP server for your AI agent.',
    long_description:
      'Hycli records a website task once, then exposes it as a local CLI command and an MCP tool. ' +
      'Reads run immediately; website changes wait for the user to approve them in the local dashboard. ' +
      'Installed website actions are generated on the user machine and are not bundled with this package.',
    author: {name: 'Hybirdss', url: 'https://github.com/Hybirdss'},
    repository: {type: 'git', url: 'https://github.com/Hybirdss/Hycli'},
    homepage: 'https://github.com/Hybirdss/Hycli',
    license: 'Apache-2.0',
    keywords: ['mcp', 'cli', 'browser', 'automation', 'agent', 'website'],
    tools_generated: true,
    compatibility: {platforms: [entry.platform]},
    server: {type: 'binary', entry_point: `server/${entry.exe}`, mcp_config: mcpConfig}
  };
}

// Resolve the mcpb CLI without depending on how npm shims `.cmd` files on Windows: prefer a local
// installation's JS entry point, then an explicit path, then PATH.
async function resolveMcpb(explicit) {
  const candidate = explicit || process.env.MCPB_CLI;
  if (candidate) {
    return /\.(?:c|m)?js$/.test(candidate)
      ? {command: process.execPath, args: [candidate]}
      : {command: candidate, args: []};
  }
  const local = path.join(root, '.cache', 'mcpb', 'node_modules', '@anthropic-ai', 'mcpb', 'dist', 'cli', 'cli.js');
  try {
    await fs.access(local);
    return {command: process.execPath, args: [local]};
  } catch {
    return {command: 'mcpb', args: []};
  }
}

function run(command, args, opts = {}) {
  const res = spawnSync(command, args, {cwd: root, encoding: 'utf8', ...opts});
  if (res.error) throw res.error;
  if (res.status !== 0) {
    throw new Error(`${command} ${args.join(' ')} failed (${res.status}): ${res.stderr || res.stdout}`);
  }
  return (res.stdout || '').trim();
}

async function require(file) {
  try {
    await fs.access(file);
  } catch {
    throw new Error(`Missing ${path.relative(root, file)} — run node scripts/package.mjs on this platform first`);
  }
}

// Download the published archive for `version` and check it against the release's own sidecar.
// A maintainer needs this to build the bundles the registry entry hashes without a Rust toolchain.
async function fetchReleaseArchive(entry, archive) {
  const base = `https://github.com/Hybirdss/Hycli/releases/download/v${version}`;
  const name = path.basename(archive);
  const sidecar = (await download(`${base}/${name}.sha256`)).toString('utf8');
  const expected = sidecar.trim().split(/\s+/)[0];
  let bytes;
  try {
    bytes = await fs.readFile(archive);
  } catch {
    console.log(`[fetch] ${name}`);
    bytes = await download(`${base}/${name}`);
    await fs.mkdir(path.dirname(archive), {recursive: true});
    await fs.writeFile(archive, bytes);
  }
  const actual = createHash('sha256').update(bytes).digest('hex');
  if (actual !== expected) throw new Error(`Release checksum mismatch for ${name}: expected ${expected}, got ${actual}`);
  console.log(`[fetch] ${name} verified ${actual.slice(0, 12)}…`);
}

async function download(url) {
  const res = await fetch(url, {redirect: 'follow'});
  if (!res.ok) throw new Error(`GET ${url} failed: ${res.status}`);
  return Buffer.from(await res.arrayBuffer());
}

// The supported release packager emits regular files/directories only. Reject a supplied
// archive that would make copying read outside the extracted public payload.
async function rejectLinks(directory) {
  for (const name of await fs.readdir(directory)) {
    const file = path.join(directory, name);
    const stat = await fs.lstat(file);
    if (stat.isDirectory()) await rejectLinks(file);
    else if (!stat.isFile()) throw new Error(`Unsupported archive entry: ${name}`);
  }
}

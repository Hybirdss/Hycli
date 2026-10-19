#!/usr/bin/env node
// Download verified CI native artifacts (Windows, macOS, Linux x64 & ARM64), verify SHA-256 sidecars, and publish to GitHub Release.
import fs from 'node:fs/promises';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {parseArgs} from 'node:util';
import {buildServerJson} from './registry-server.mjs';

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const {values} = parseArgs({
  options: {
    'run-id': {type: 'string'},
    tag: {type: 'string'},
    'dry-run': {type: 'boolean'},
    help: {type: 'boolean'}
  }
});

if (values.help) {
  console.log('Usage: node scripts/publish-release.mjs [--run-id <github-run-id>] [--tag <v0.1.0>] [--dry-run]');
  process.exit(0);
}

function run(command, args, opts = {}) {
  const res = spawnSync(command, args, {cwd: root, encoding: 'utf8', ...opts});
  if (res.error) throw res.error;
  if (res.status !== 0) throw new Error(`${command} ${args.join(' ')} failed (${res.status}): ${res.stderr || res.stdout}`);
  return (res.stdout || '').trim();
}

const cargoToml = await fs.readFile(path.join(root, 'Cargo.toml'), 'utf8');
const version = cargoToml.match(/^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"/m)?.[1];
if (!version) throw new Error('Missing version in Cargo.toml');
const tag = values.tag || `v${version}`;

const runId = values['run-id'] || JSON.parse(
  run('gh', ['run', 'list', '--workflow', 'verify.yml', '--branch', 'main', '--status', 'success', '--limit', '1', '--json', 'databaseId'])
)[0]?.databaseId;
if (!runId) throw new Error('No successful Verify and package run found on main');

const baseTmp = path.join(root, '.cache');
await fs.mkdir(baseTmp, {recursive: true});
const stage = await fs.mkdtemp(path.join(baseTmp, `hycli-release-${runId}-`));

const expectedPlatforms = [
  'hycli-win32-x64',
  'hycli-win32-arm64',
  'hycli-darwin-arm64',
  'hycli-darwin-x64',
  'hycli-linux-x64',
  'hycli-linux-arm64'
];

try {
  console.log(`Downloading artifacts from workflow run ${runId} into ${stage}...`);
  run('gh', ['run', 'download', String(runId), '--dir', stage], {stdio: 'inherit'});

  const collected = new Map();
  for (const name of expectedPlatforms) {
    const dir = path.join(stage, name);
    const entries = await fs.readdir(dir);
    for (const entry of entries) {
      const fullPath = path.join(dir, entry);
      const stat = await fs.stat(fullPath);
      if (stat.isFile()) {
        collected.set(entry, fullPath);
      }
    }
  }

  const shaEntries = [];
  for (const [name, fullPath] of [...collected.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
    if (!name.endsWith('.sha256')) continue;
    const targetName = name.slice(0, -'.sha256'.length);
    const targetPath = collected.get(targetName);
    if (!targetPath) throw new Error(`Missing target artifact for checksum sidecar: ${name}`);
    const sidecar = (await fs.readFile(fullPath, 'utf8')).trim();
    const expectedHash = sidecar.split(/\s+/)[0];
    const actualHash = createHash('sha256').update(await fs.readFile(targetPath)).digest('hex');
    if (expectedHash !== actualHash) {
      throw new Error(`SHA-256 mismatch for ${targetName}: expected ${expectedHash}, got ${actualHash}`);
    }
    shaEntries.push(`${actualHash}  ${targetName}`);
  }

  const shaSumsPath = path.join(stage, 'SHA256SUMS');
  await fs.writeFile(shaSumsPath, shaEntries.join('\n') + '\n');

  // The registry entry must hash the exact bytes the release serves, so server.json is built from
  // the collected bundles here instead of being committed with hashes measured somewhere else.
  const bundles = [];
  for (const [name, fullPath] of [...collected.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
    if (!name.endsWith('.mcpb')) continue;
    bundles.push({file: name, sha256: createHash('sha256').update(await fs.readFile(fullPath)).digest('hex')});
  }
  const expectedBundles = ['linux-x64', 'linux-arm64', 'darwin-x64', 'darwin-arm64', 'win32-x64', 'win32-arm64'];
  const missing = expectedBundles.filter(id => !bundles.some(bundle => bundle.file === `hycli-mcp-${version}-${id}.mcpb`));
  if (missing.length) throw new Error(`Missing MCP bundles: ${missing.join(', ')}`);

  const serverJsonPath = path.join(stage, 'server.json');
  await fs.writeFile(serverJsonPath, JSON.stringify(buildServerJson({version, bundles, tag}), null, 2) + '\n');
  const uploadFiles = [...collected.values(), shaSumsPath, serverJsonPath];

  console.log(JSON.stringify({
    run_id: runId,
    tag,
    verified_artifacts: shaEntries.length,
    mcp_bundles: bundles.length,
    total_files: uploadFiles.length
  }));

  if (!values['dry-run']) {
    run('gh', ['release', 'upload', tag, ...uploadFiles, '--clobber'], {stdio: 'inherit'});
  }
} finally {
  await fs.rm(stage, {recursive: true, force: true});
}

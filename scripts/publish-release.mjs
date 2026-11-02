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
if (!/^[1-9][0-9]*$/.test(String(runId))) throw new Error('Invalid workflow run ID');
const workflowRun = JSON.parse(run('gh', ['api', `repos/{owner}/{repo}/actions/runs/${runId}`]));
if (String(workflowRun.id) !== String(runId) || workflowRun.status !== 'completed' || workflowRun.conclusion !== 'success' ||
    workflowRun.path !== '.github/workflows/verify.yml' || workflowRun.head_branch !== 'main' ||
    !['push', 'workflow_dispatch'].includes(workflowRun.event)) {
  throw new Error('Release requires a completed successful Verify and package run on main');
}
const taggedCommit = JSON.parse(run('gh', ['api', `repos/{owner}/{repo}/commits/${encodeURIComponent('refs/tags/' + tag)}`]));
if (!/^[a-f0-9]{40}$/.test(taggedCommit.sha) || workflowRun.head_sha !== taggedCommit.sha) {
  throw new Error('Workflow source SHA does not match the requested release tag');
}
const taggedCargo = JSON.parse(run('gh', ['api', `repos/{owner}/{repo}/contents/Cargo.toml?ref=${taggedCommit.sha}`]));
if (taggedCargo.encoding !== 'base64' || typeof taggedCargo.content !== 'string') throw new Error('Missing tagged Cargo.toml content');
const taggedVersion = Buffer.from(taggedCargo.content, 'base64').toString('utf8').match(/^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"/m)?.[1];
if (taggedVersion !== version) throw new Error(`Release tag version does not match Cargo.toml: expected ${version}, got ${taggedVersion}`);

const expectedPlatforms = [
  'hycli-win32-x64',
  'hycli-win32-arm64',
  'hycli-darwin-arm64',
  'hycli-darwin-x64',
  'hycli-linux-x64',
  'hycli-linux-arm64'
];
const artifacts = [];
for (const folder of expectedPlatforms) {
  const id = folder.slice('hycli-'.length), [platform, arch] = id.split('-');
  const stem = `hycli-${version}-${id}`;
  artifacts.push(
    {folder, name: stem + '.tar.gz', manifest: {name: stem + '.manifest.json', kind: 'native', platform, arch}},
    {folder, name: `hycli-mcp-${version}-${id}.mcpb`},
    {folder, name: stem + ({linux: '.deb', darwin: '.dmg', win32: '-setup.exe'}[platform])}
  );
}
artifacts.push({folder: 'hycli-linux-x64', name: `hycli-${version}-source.tar.gz`,
  manifest: {name: `hycli-${version}-source.manifest.json`, kind: 'source', platform: null, arch: null}});
const expectedFiles = new Map();
for (const artifact of artifacts) {
  for (const name of [artifact.name, artifact.name + '.sha256', ...(artifact.manifest ? [artifact.manifest.name] : [])]) {
    expectedFiles.set(name, artifact.folder);
  }
}
const baseTmp = path.join(root, '.cache');
await fs.mkdir(baseTmp, {recursive: true});
const stage = await fs.mkdtemp(path.join(baseTmp, `hycli-release-${runId}-`));

try {
  console.log(`Downloading artifacts from workflow run ${runId} into ${stage}...`);
  run('gh', ['run', 'download', String(runId), '--dir', stage], {stdio: 'inherit'});

  const collected = new Map();
  for (const folder of expectedPlatforms) {
    const dir = path.join(stage, folder);
    const directory = await fs.lstat(dir).catch(error => {
      if (error.code === 'ENOENT') throw new Error(`Missing artifact directory: ${folder}`);
      throw error;
    });
    if (!directory.isDirectory()) throw new Error(`Invalid artifact directory: ${folder}`);
    for (const name of await fs.readdir(dir)) {
      if (collected.has(name)) throw new Error(`Duplicate artifact basename: ${name}`);
      const fullPath = path.join(dir, name);
      if (!(await fs.lstat(fullPath)).isFile()) throw new Error(`Artifact must be a regular file: ${name}`);
      collected.set(name, fullPath);
    }
  }
  for (const [name, folder] of expectedFiles) {
    if (!collected.has(name)) throw new Error(`Missing required artifact: ${name}`);
    if (collected.get(name) !== path.join(stage, folder, name)) throw new Error(`Artifact is in the wrong platform directory: ${name}`);
  }
  for (const name of collected.keys()) {
    if (!expectedFiles.has(name)) throw new Error(`Unexpected artifact: ${name}`);
  }

  const hashes = new Map();
  const shaEntries = [];
  for (const artifact of [...artifacts].sort((a, b) => a.name.localeCompare(b.name))) {
    const sidecar = (await fs.readFile(collected.get(artifact.name + '.sha256'), 'utf8')).trim();
    const match = sidecar.match(/^([a-f0-9]{64})  ([^\r\n]+)$/);
    if (!match || match[2] !== artifact.name) throw new Error(`Invalid checksum sidecar: ${artifact.name}.sha256`);
    const actualHash = createHash('sha256').update(await fs.readFile(collected.get(artifact.name))).digest('hex');
    if (match[1] !== actualHash) throw new Error(`SHA-256 mismatch for ${artifact.name}: expected ${match[1]}, got ${actualHash}`);
    hashes.set(artifact.name, actualHash);
    shaEntries.push(`${actualHash}  ${artifact.name}`);
    if (artifact.manifest) {
      const expected = artifact.manifest;
      const manifest = JSON.parse(await fs.readFile(collected.get(expected.name), 'utf8'));
      if (!manifest || manifest.version !== version || manifest.kind !== expected.kind ||
          manifest.platform !== expected.platform || manifest.arch !== expected.arch || manifest.sha256 !== actualHash ||
          !Array.isArray(manifest.files) || !manifest.files.length ||
          !manifest.files.every(file => typeof file === 'string' && file.length > 0) ||
          new Set(manifest.files).size !== manifest.files.length) {
        throw new Error(`Invalid archive manifest: ${expected.name}`);
      }
    }
  }

  const shaSumsPath = path.join(stage, 'SHA256SUMS');
  await fs.writeFile(shaSumsPath, shaEntries.join('\n') + '\n');
  const bundles = artifacts.filter(artifact => artifact.name.endsWith('.mcpb'))
    .map(artifact => ({file: artifact.name, sha256: hashes.get(artifact.name)}));

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

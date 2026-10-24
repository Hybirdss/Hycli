import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const guide = 'docs/ACCOUNT_READ.md';

async function fixture(t, includeGuide = true) {
  const scratch = path.join(root, '.cache', 'mcpb-guidance-tests');
  await fs.mkdir(scratch, {recursive: true});
  const dir = await fs.mkdtemp(path.join(scratch, 'case with spaces-'));
  t.after(() => fs.rm(dir, {recursive: true, force: true}));
  await fs.mkdir(path.join(dir, 'scripts'));
  for (const file of ['mcpb.mjs', 'registry-server.mjs']) {
    await fs.copyFile(path.join(root, 'scripts', file), path.join(dir, 'scripts', file));
  }
  await fs.writeFile(path.join(dir, 'Cargo.toml'), '[package]\nversion = "0.1.0"\n');
  const source = path.join(dir, 'input', 'package');
  const files = {
    hycli: 'synthetic executable',
    LICENSE: 'synthetic license',
    'README.md': 'synthetic readme',
    'docs/unrelated.md': 'not part of the public bundle contract',
    ...(includeGuide ? {[guide]: await fs.readFile(path.join(root, guide), 'utf8')} : {}),
  };
  for (const file of ['agent/AGENTS.md', 'skills/hycli/SKILL.md', 'skills/hycli/references/agent-guide.md']) {
    files[file] = includeGuide ? await fs.readFile(path.join(root, file), 'utf8') : 'Legacy guidance without account reads.\n';
    if (includeGuide) assert.ok(files[file].includes(guide), `${file} must reference the packaged guide`);
  }
  for (const [file, content] of Object.entries(files)) {
    await fs.mkdir(path.dirname(path.join(source, file)), {recursive: true});
    await fs.writeFile(path.join(source, file), content);
  }
  await fs.mkdir(path.join(dir, 'assets'));
  const env = {...process.env};
  delete env.TAR_OPTIONS;
  const packed = spawnSync('tar', ['-czf', '-', 'package'], {cwd: path.dirname(source), env});
  assert.equal(packed.status, 0, String(packed.stderr));
  await fs.writeFile(path.join(dir, 'assets', 'hycli-0.1.0-linux-x64.tar.gz'), packed.stdout);
  await fs.writeFile(path.join(dir, 'capture-mcpb.mjs'), `
import fs from 'node:fs';
import path from 'node:path';
const [verb, stage, output] = process.argv.slice(2);
if (verb === 'validate') JSON.parse(fs.readFileSync(stage));
else if (verb === 'pack') {
  const files = {};
  function collect(directory, prefix = '') {
    for (const entry of fs.readdirSync(directory, {withFileTypes: true})) {
      const name = prefix + entry.name, file = path.join(directory, entry.name);
      if (entry.isDirectory()) collect(file, name + '/');
      else files[name] = fs.readFileSync(file, 'utf8');
    }
  }
  collect(stage);
  fs.writeFileSync(output, JSON.stringify(files));
} else throw Error('Unexpected command');
`);
  const result = spawnSync(process.execPath, [path.join(dir, 'scripts', 'mcpb.mjs'),
    '--assets', 'assets', '--output', 'output', '--mcpb', path.join(dir, 'capture-mcpb.mjs'),
    '--keep-stage'], {cwd: dir, encoding: 'utf8', timeout: 30000});
  return {dir, files, result};
}

test('MCP bundle packaging preserves the referenced offline guide and excludes unrelated documents', async t => {
  const {dir, files, result} = await fixture(t);
  assert.equal(result.status, 0, result.stderr);
  const payload = JSON.parse(await fs.readFile(path.join(dir, 'output', 'hycli-mcp-0.1.0-linux-x64.mcpb')));
  assert.equal(payload[guide], files[guide], 'The bundled guidance must include its referenced descriptor guide');
  assert.equal(await fs.readFile(path.join(dir, 'dist/mcpb/stage/hycli-mcp-0.1.0-linux-x64', guide), 'utf8'), files[guide]);
  assert.ok(!('docs/unrelated.md' in payload));
});

test('MCP bundle packaging remains compatible with older archives without the guide', async t => {
  const {dir, result} = await fixture(t, false);
  assert.equal(result.status, 0, result.stderr);
  const payload = JSON.parse(await fs.readFile(path.join(dir, 'output', 'hycli-mcp-0.1.0-linux-x64.mcpb')));
  assert.ok(!Object.keys(payload).some(file => file.startsWith('docs/')));
});

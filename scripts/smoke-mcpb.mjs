#!/usr/bin/env node
// Execute the final native MCPB, not the source tree or its input tarball.
import fs from 'node:fs/promises';
import path from 'node:path';
import {spawn, spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const archive = process.argv[2] && path.resolve(process.argv[2]);
if (!archive) throw new Error('Usage: node scripts/smoke-mcpb.mjs <native.mcpb>');
if (!path.basename(archive).endsWith(`-${process.platform}-${process.arch}.mcpb`)) {
  throw new Error('MCP bundle must match this native runner platform and architecture');
}
const bytes = await fs.readFile(archive);
const hash = createHash('sha256').update(bytes).digest('hex');
const expected = (await fs.readFile(archive + '.sha256', 'utf8')).trim().split(/\s+/)[0];
if (hash !== expected) throw new Error('MCP bundle checksum mismatch');
const scratch = path.join(root, '.cache', 'mcpb-smoke');
await fs.mkdir(scratch, {recursive: true});
const directory = await fs.mkdtemp(path.join(scratch, 'fresh-'));
const unpacked = path.join(directory, 'bundle');
const timeout = Number(process.env.HYCLI_SMOKE_TIMEOUT_MS) || 30000;
let mcp;
const pending = new Map();
function fail(error) {
  for (const request of pending.values()) {
    clearTimeout(request.timer);
    request.reject(error);
  }
  pending.clear();
}
try {
  // CI installs this exact tool version before packaging and smoke testing.
  const cli = path.join(root, '.cache/mcpb/node_modules/@anthropic-ai/mcpb/dist/cli/cli.js');
  const unpack = spawnSync(process.execPath, [cli, 'unpack', archive, unpacked], {encoding: 'utf8', timeout});
  if (unpack.error) throw unpack.error;
  if (unpack.status !== 0) throw new Error(`MCPB unpack failed: ${unpack.stderr || unpack.stdout}`);
  const manifest = JSON.parse(await fs.readFile(path.join(unpacked, 'manifest.json'), 'utf8'));
  const entry = `server/hycli${process.platform === 'win32' ? '.exe' : ''}`;
  if (manifest.server.type !== 'binary' || manifest.server.entry_point !== entry ||
      !manifest.compatibility.platforms.includes(process.platform)) throw new Error('Unexpected native manifest');
  const config = {...manifest.server.mcp_config, ...manifest.server.mcp_config.platform_overrides?.[process.platform]};
  if (config.command !== `\${__dirname}/${entry}` || JSON.stringify(config.args) !== '["mcp"]') {
    throw new Error('Unexpected MCP command: the manifest must launch the packaged binary with mcp');
  }
  const executable = config.command.replace('${__dirname}', unpacked);
  const home = path.join(directory, 'home');
  const cwd = path.join(directory, 'outside-checkout');
  await fs.mkdir(home);
  await fs.mkdir(cwd);
  // Do not inherit provider keys, installed definitions, or the developer's profile.
  const env = {
    PATH: path.dirname(executable), HOME: home, USERPROFILE: home,
    APPDATA: path.join(home, 'roaming'), LOCALAPPDATA: path.join(home, 'local'),
    XDG_DATA_HOME: path.join(home, 'data'), XDG_CONFIG_HOME: path.join(home, 'config'),
    XDG_CACHE_HOME: path.join(home, 'cache'), HYCLI_DATA_DIR: path.join(directory, 'data'),
    TMPDIR: directory, TMP: directory, TEMP: directory, LANG: 'C.UTF-8'
  };
  for (const key of ['SystemRoot', 'SYSTEMROOT', 'WINDIR']) {
    if (process.env[key]) env[key] = process.env[key];
  }
  const version = spawnSync(executable, ['--version'], {cwd, env, encoding: 'utf8', timeout});
  if (version.error) throw version.error;
  if (version.status !== 0 || version.stdout.trim() !== `hycli ${manifest.version}`) {
    throw new Error(`Packaged executable version failed: ${version.stderr || version.stdout}`);
  }
  mcp = spawn(executable, config.args, {cwd, env, stdio: ['pipe', 'pipe', 'pipe']});
  let buffer = '', stderr = '', sequence = 0;
  mcp.stderr.on('data', chunk => { stderr = (stderr + chunk).slice(-8192); });
  mcp.on('error', fail);
  mcp.on('exit', code => fail(new Error(`Packaged MCP exited ${code}: ${stderr}`)));
  mcp.stdin.on('error', fail);
  mcp.stdout.on('data', chunk => {
    buffer += chunk;
    while (buffer.includes('\n')) {
      const end = buffer.indexOf('\n'), line = buffer.slice(0, end);
      buffer = buffer.slice(end + 1);
      if (!line.trim()) continue;
      try {
        const message = JSON.parse(line), request = pending.get(message.id);
        if (request) {
          clearTimeout(request.timer);
          pending.delete(message.id);
          message.error ? request.reject(new Error(message.error.message)) : request.resolve(message.result);
        }
      } catch (error) { fail(error); }
    }
  });
  const rpc = (method, params) => new Promise((resolve, reject) => {
    const id = ++sequence;
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`MCP timeout: ${method}: ${stderr}`)); }, timeout);
    pending.set(id, {resolve, reject, timer});
    mcp.stdin.write(JSON.stringify({jsonrpc: '2.0', id, method, params}) + '\n');
  });
  await rpc('initialize', {protocolVersion: '2025-03-26', capabilities: {}, clientInfo: {name: 'mcpb-smoke', version: '1'}});
  mcp.stdin.write(JSON.stringify({jsonrpc: '2.0', method: 'notifications/initialized'}) + '\n');
  const {tools} = await rpc('tools/list', {});
  for (const name of ['hycli_prepare', 'hycli_run', 'hycli_sites', 'hycli_accounts']) {
    if (!tools.some(tool => tool.name === name)) throw new Error(`Missing tool: ${name}`);
  }
  const sites = await rpc('tools/call', {name: 'hycli_sites', arguments: {}});
  if (sites.isError || JSON.stringify(JSON.parse(sites.content.find(item => item.type === 'text').text)) !== '[]') {
    throw new Error('Fresh MCP bundle contains installed websites');
  }
  console.log(JSON.stringify({bundle: path.basename(archive), sha256: hash, platform: process.platform,
    arch: process.arch, version: manifest.version, extraction: 'passed', mcp: 'passed', tools: tools.length, fresh_sites: 'empty'}));
} finally {
  fail(new Error('MCP smoke finished'));
  if (mcp && mcp.exitCode === null && mcp.signalCode === null) {
    await new Promise((resolve, reject) => {
      const killTimer = setTimeout(() => mcp.kill('SIGKILL'), 5000);
      const deadline = setTimeout(() => reject(new Error('MCP process did not stop; scratch directory retained')), 10000);
      mcp.once('close', () => { clearTimeout(killTimer); clearTimeout(deadline); resolve(); });
      mcp.kill('SIGTERM');
    });
  }
  await fs.rm(directory, {recursive: true, force: true});
}

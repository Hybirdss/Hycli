// Registry metadata shared by the bundle builder and the release publisher.
// The official MCP registry stores metadata only; `packages[].fileSha256` must describe the exact
// bytes served by the GitHub release, so this module never computes a hash itself.
import fs from 'node:fs/promises';
import path from 'node:path';

export const REPOSITORY_URL = 'https://github.com/Hybirdss/Hycli';
// GitHub's stable numeric repository id (survives renames; the registry uses it to detect resurrection).
export const REPOSITORY_ID = '1412178580';

export async function readVersion(root) {
  const cargoToml = await fs.readFile(path.join(root, 'Cargo.toml'), 'utf8');
  const found = cargoToml.match(/^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"/m)?.[1];
  if (!found) throw new Error('Missing version in Cargo.toml');
  return found;
}

// `bundles` are the released MCP bundles in upload order: [{file: 'hycli-mcp-0.1.0-linux-x64.mcpb', sha256}].
export function buildServerJson({version, bundles, tag = `v${version}`}) {
  if (!bundles.length) throw new Error('No MCP bundles to publish');
  for (const bundle of bundles) {
    if (!/^[a-f0-9]{64}$/.test(bundle.sha256)) throw new Error(`Invalid sha256 for ${bundle.file}`);
    if (!bundle.file.includes('mcp')) throw new Error(`Bundle name must contain "mcp": ${bundle.file}`);
  }
  return {
    $schema: 'https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json',
    name: 'io.github.Hybirdss/hycli',
    title: 'Hycli',
    websiteUrl: `${REPOSITORY_URL}#use-hycli-with-your-ai`,
    description: 'Turn any website into a typed CLI and MCP server for your AI agent.',
    repository: {url: REPOSITORY_URL, source: 'github', id: REPOSITORY_ID},
    version,
    packages: bundles.map(bundle => ({
      registryType: 'mcpb',
      identifier: `${REPOSITORY_URL}/releases/download/${encodeURIComponent(tag)}/${bundle.file}`,
      fileSha256: bundle.sha256,
      transport: {type: 'stdio'}
    }))
  };
}

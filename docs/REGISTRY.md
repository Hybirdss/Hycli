# Publish Hycli to the official MCP registry

The official registry (`registry.modelcontextprotocol.io`) stores metadata only. Installation files stay on the GitHub release, and the registry entry carries the SHA-256 of each file. Hycli is a native binary, so the MCP bundle (`mcpb`) package type is the only supported channel that does not make users compile or install a runtime: npm and cargo entries would ask for a Rust toolchain and a Chromium dependency, and an OCI image cannot hold the browser sign-in state the product needs.

## What is registered

| Field | Value |
| --- | --- |
| `name` | `io.github.Hybirdss/hycli` |
| `registryType` | `mcpb` (one entry per platform) |
| `identifier` | `https://github.com/Hybirdss/Hycli/releases/download/v<version>/hycli-mcp-<version>-<os>-<arch>.mcpb` |
| `transport` | `stdio` |
| `repository` | GitHub, numeric id `1412178580` |

The registry entry is `server.json`: `name`, `title`, `description`, `repository`, `version` and one `packages[]` entry per platform with `fileSha256`. It is **generated**, never committed: the file hashes the bytes the release serves, and packing is not reproducible (three runs over identical input produced three different SHA-256 values), so a committed hash snapshot would silently stop matching. `.gitignore` excludes `/packaging/registry/server.json` for that reason; the release publisher uploads the generated file next to `SHA256SUMS`.

## Build the bundles

The input is the packaged archive that users download, not a fresh source build.

```sh
node scripts/build.mjs
node scripts/package.mjs                      # dist/artifacts/hycli-<version>-<platform>.tar.gz
npm install --prefix .cache/mcpb --no-save "@anthropic-ai/mcpb@2.1.2"
node scripts/mcpb.mjs                         # every platform found in dist/artifacts
node scripts/mcpb.mjs --platform linux-x64    # one platform
```

Without a Rust toolchain, build from the published release instead — `--fetch` downloads `hycli-<version>-<platform>.tar.gz` and checks it against the release's own `.sha256` sidecar before packing:

```sh
node scripts/mcpb.mjs --fetch --server-json packaging/registry/server.json
```

`scripts/mcpb.mjs` extracts each archive, writes a manifest, runs `mcpb validate`, packs the bundle, and records the result.

| Output | Meaning |
| --- | --- |
| `dist/artifacts/hycli-mcp-<version>-<os>-<arch>.mcpb` | bundle uploaded with the release |
| `dist/artifacts/hycli-mcp-<version>-<os>-<arch>.mcpb.sha256` | sidecar in the same `hash  name` form as the other artifacts |
| `dist/mcpb/mcpb-report.json` | platform, size, SHA-256 and entry point for every bundle |

`--server-json packaging/registry/server.json` additionally writes the registry manifest from the measured hashes.

A bundle contains `manifest.json`, `server/hycli` (or `server/hycli.exe`), `LICENSE`, `README.md`, `agent/` and `skills/hycli/`. It never contains installed site definitions, accounts, browser profiles or local databases.

### Manifest decisions

- `server.type` is `binary`, `entry_point` is `server/hycli` (`server/hycli.exe` on Windows), and `mcp_config.command` is `${__dirname}/server/hycli` with `args: ["mcp"]` — the same command a user would run by hand.
- Windows bundles repeat the `.exe` command in `platform_overrides.win32` so a client that prefers an override over the base command launches the same binary.
- `compatibility.platforms` names the single platform of the bundle. MCPB has no architecture field, so x64 and ARM64 ship as separate bundles and the identifier URL carries the architecture.
- `tools_generated` is `true`: website actions are generated on the user's machine. A fresh install exposes the ten built-in tools (`hycli_sites`, `hycli_run`, `hycli_prepare`, `hycli_request`, `hycli_recon`, `hycli_check`, `hycli_result`, `hycli_job`, `hycli_activity`, `hycli_accounts`).
- No `user_config` entry: sign-in state lives in the platform data directory, never in the manifest.

## Verification

Measured 2026-10-19 on the release binaries of v0.1.0:

| Check | Result |
| --- | --- |
| `mcpb validate manifest.json` | passes for all six platforms |
| `mcp-publisher validate packaging/registry/server.json` (six packages) | `✅ server.json is valid`, exit 0 |
| same file reduced to three packages (darwin-arm64, win32-x64, linux-x64) | also valid — six entries are allowed, so the full set is published |
| `fileSha256` replaced with `deadbeef` | HTTP 422 `expected string to match pattern ^[a-f0-9]{64}$` — validation is server-side, not a local no-op |
| identifier renamed to a name without `mcp` | accepted; the registry does not enforce the `mcp` substring, the `-mcp-` name is kept because it distinguishes the bundle from the tarball |
| unpacked bundle, `server/hycli mcp` over stdio | `initialize` + `tools/list` answered, `serverInfo` `rmcp 3.5.1`, ten tools |
| macOS and Windows binaries | not executed here — no such host. Label them unverified until CI or a maintainer runs them |

### Linux runtime floor (measured)

The linux-x64 release binary requires glibc 2.38 (`GLIBC_2.38` not found on Debian 12's 2.36; a weak `GLIBC_2.39` reference makes 2.39 the practical target). Debian 12 and Ubuntu 22.04 users cannot run the current linux-x64 build, bundle or tarball. Users on those distributions need a build against an older glibc before the Linux entry is useful to them.

## Publishing (maintainer)

Bundles must be on the release before publishing, because the registry only stores the link and the hash. Three steps, in this order:

1. Build the bundles from the published archives and write the entry: `node scripts/mcpb.mjs --fetch --server-json packaging/registry/server.json`
2. Upload exactly those bytes: `gh release upload v0.1.0 dist/artifacts/hycli-mcp-0.1.0-*.mcpb dist/artifacts/hycli-mcp-0.1.0-*.mcpb.sha256 packaging/registry/server.json --clobber`
3. `mcp-publisher login github` (browser OAuth, maintainer account — an agent cannot complete it), then `mcp-publisher publish packaging/registry/server.json`

Verify the result, and check that a client would download a file matching the stored hash:

```sh
curl -s "https://registry.modelcontextprotocol.io/v0.1/servers?search=hycli"
curl -sL "https://github.com/Hybirdss/Hycli/releases/download/v0.1.0/hycli-mcp-0.1.0-linux-x64.mcpb" | sha256sum
```

When CI published the release, prefer the `server.json` that `scripts/publish-release.mjs` uploaded: it hashed the bundles it collected from CI, so it matches those bytes by construction.

### Rollback

```sh
mcp-publisher status --status deleted --message "Reason" io.github.Hybirdss/hycli 0.1.0
mcp-publisher status --status active io.github.Hybirdss/hycli 0.1.0
```

`deprecated` keeps the entry visible with a message; `deleted` removes the version; `active` restores it. `--all-versions` applies the change to every version.

## Hashes are per build

Packing is not reproducible: three runs over identical input produced three different SHA-256 values, and normalizing every staged file to a fixed mtime did not change that — the difference comes from the packer's zip writer, not from the staged files. Consequences:

- Never regenerate a bundle after uploading it — the registry hash would stop matching the file.
- Regenerate `server.json` from the uploaded files, not from a local rebuild.
- A version bump needs new bundles, a new release and a new `server.json`; the registry keeps one entry per version.

## CI wiring

- `.github/workflows/verify.yml` packs the bundle of each platform right after `scripts/package.mjs`, so `dist/artifacts/` carries the `.mcpb` and its sidecar into the workflow artifact.
- `scripts/publish-release.mjs` refuses to publish when any of the six bundles is missing, builds `server.json` from the collected bytes, and uploads it with `SHA256SUMS`.

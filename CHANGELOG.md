# Changelog

## Unreleased

- Open Hycli from a native app icon or `hycli open` without keeping a terminal or manually starting a server. Reuse running instances and select a free local port when necessary.
- Quit from Settings in all 20 languages, or use `hycli status` / `hycli stop`. Preserve active work and saved data.
- Build per-user Windows installers, macOS app/DMG packages and Linux DEBs, plus portable archives with a Linux menu installer, for x64 and ARM64.
- Verify background lifecycle and actual installer payloads in each native CI job. Support optional Windows signing and macOS Developer ID signing/notarization.

## 0.1.0

The first packaged Hycli release: websites, ready for AI.

### Website tools

- Prepare typed CLI and MCP actions from a website URL and an optional task.
- Use REST, JSON and YAML OpenAPI, GraphQL, URL-encoded forms, HTML extraction and rendered Chromium GET reads.
- Follow three preparation workers, verify useful reads and repair definitions using observed failures.
- Keep the existing working definition while preparing a replacement.

### Your local workspace

- Manage websites, accounts, provider connections, results and approvals from the embedded dashboard.
- Connect multiple accounts and import available browser sessions, origin storage or API credentials.
- Connect AI assistants through MCP, with generic execution for newly installed actions.
- Use 20 dashboard languages, including right-to-left Arabic, with matching README entry points.

### Packaging and verification

- Ship a single native executable with the dashboard and instructions embedded.
- Publish a verified Linux x64 archive and a source snapshot with manifests and SHA-256 checksums.
- Exercise CLI, MCP, dashboard, lifecycle, provider, layout and browser-companion flows against isolated fixtures.
- Define native x64 and ARM64 CI jobs for Linux, macOS and Windows; consult the actual workflow results for platform verification.

The Linux download requires glibc 2.39 or newer. Arbitrary browser click sequences, multipart uploads and binary downloads are outside the current operation contract. See [SITESPEC.md](docs/SITESPEC.md) for supported behavior and [the release](https://github.com/Hybirdss/Hycli/releases/tag/v0.1.0) for downloadable assets.

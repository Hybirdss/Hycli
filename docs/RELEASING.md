# Release a public Hycli snapshot

A release includes the native executable, license, end-user agent instructions, portable skill and public documentation. User data and developer state are never release inputs.

## Verify and package

Run the checks in [BUILD.md](BUILD.md), then:

```sh
node scripts/build.mjs
node scripts/package.mjs
node scripts/package.mjs --source
node scripts/smoke-package.mjs dist/artifacts/hycli-0.1.0-linux-x64.tar.gz
```

Use the archive matching the current native platform for the last command. Archives and their SHA-256 sidecars are under `dist/artifacts/`. Each archive also includes checksums for its individual files. `*.manifest.json` records the exact packaged file list.

Packaging copies only the declared public files. It does not archive the entire `dist/` folder, Git index, working tree or Git history. The source snapshot omits installed site definitions, saved accounts, browser profiles, local databases, credentials, debug executables, tool state and private development records. Unknown files at the project root are not automatically included.

Packaging requires GNU tar or bsdtar (libarchive). Archive ownership is normalized to UID/GID 0, and host ACLs, extended attributes and macOS metadata are omitted. The archive does not retain the developer's OS username or group name.

The smoke script extracts the package into a new directory, checks every checksum, starts the CLI and embedded dashboard with fresh data, loads the built assets, initializes MCP and checks that no sites or accounts are preinstalled. The runtime PATH contains only the extracted package directory. Node and the archive utility are test tools, not runtime dependencies.

## Native CI and publication

The verification workflow defines native x64 and ARM64 jobs on Linux, macOS and Windows. Each job builds its own executable and checks its extracted package. Linux x64 additionally runs the full browser fixture suite. A workflow definition is not evidence that a runner passed: use the actual CI result for each published platform.

Run the workflow on the intended public repository and review its artifacts before creating a public release. Publication requires a real repository and permission to publish there. This repository does not assume that a developer's remote or account is the user's destination.

If development history contains private paths or old generated artifacts, publish the reviewed source snapshot from a new clean checkout/history. Deleting a file in a later commit does not erase it from older commits. Keep any private development history locally; do not rewrite or push it merely to produce the public snapshot.

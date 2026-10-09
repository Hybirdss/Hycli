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

## Desktop installers and signing

Run `node scripts/package-desktop.mjs` after the release build on each target OS. It creates Windows per-user NSIS installers, macOS app bundles inside DMGs, and Linux DEBs. The portable archive contains both binaries; Linux archives also include a per-user `install.sh`. All artifacts have SHA-256 sidecars. The native CI matrix runs installer payload checks and background lifecycle checks for x64 and ARM64. Its jobs do not publish a GitHub release automatically.

The Windows installer adds Start Menu and desktop shortcuts, including a **Quit Hycli** Start Menu entry, and an uninstall entry. It requires no administrator privileges and does not modify PATH. Setup and uninstall stop an idle existing engine; busy work blocks replacement. Uninstall preserves user data. For CLI use, add `%LOCALAPPDATA%\Programs\Hycli` to your PATH or use the path shown in Coding agents.

On macOS, drag `Hycli.app` to Applications. The app opens the browser without a persistent Dock window. CLI/MCP use the bundled `Contents/MacOS/hycli`; Coding agents supplies the full path. Quit Hycli before replacing or removing the app. Removing the app retains user data.

For a trusted public macOS release, import a Developer ID Application certificate into the build keychain and set `MACOS_SIGN_IDENTITY`. Each nested binary and then the app are signed with hardened runtime and a timestamp. Set `MACOS_NOTARY_PROFILE` to an existing `notarytool` keychain profile to submit, wait for notarization, staple and validate the DMG. Without these settings, the build is ad-hoc signed for testing and is **not a notarized public release**. Do not describe it as one or ask users to disable Gatekeeper.

For Windows Authenticode signing, provision an appropriate signing certificate in the signing environment and set `WINDOWS_SIGNTOOL` to the trusted `signtool.exe` path. The packager signs both executables and the installer with SHA-256 and a timestamp. Without it, installers remain unsigned and may show Windows trust prompts. Certificate provisioning is separate from a repository build; never commit signing keys or passwords.

Linux DEBs install binaries under `/usr/lib/hycli`, a `/usr/bin/hycli` CLI link, icon and desktop entry. They install no root service. Quit Hycli before upgrading. The portable `install.sh` installs for the current user under `~/.local/lib/hycli`, adds an app-menu entry and a `~/.local/bin/hycli` link. It keeps saved data in the same platform data directory.

Review the extracted contents of **both** the portable archive and native installer. Run `scripts/smoke-installer.mjs` on the exact artifact that will be distributed. Sign and notarize before computing final published checksums. Release notes must distinguish each platform actually tested, Linux's actual glibc floor, and the signing/notarization status.

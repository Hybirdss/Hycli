# Build and run Hycli

Hycli runs locally. The release executable includes the dashboard and website engine; Node.js and Rust are build tools, not runtime requirements.

Native packages still use the target operating system's libraries. Linux GNU builds require compatible glibc and libgcc runtimes; the Ubuntu 24.04 CI build targets a glibc 2.39 baseline. An archive built on another distribution can require a newer baseline. Check the package's build platform before installing; build from source for older distributions or musl-only systems such as Alpine. macOS and Windows packages must match the machine's operating system and architecture.

## Build from a checkout

Use Rust 1.98 or newer and Node.js 20.19+ or 22.12+. The lockfiles select the Rust and JavaScript dependencies.

The standard HTTP transport uses Rustls with AWS-LC. A native C/C++ compiler is required; CMake covers the TLS library's fallback build path. The locked dependencies use generated bindings and do not require Perl or libclang. On Debian/Ubuntu:

```sh
sudo apt-get install build-essential cmake pkg-config git dpkg-dev binutils
```

On macOS, install Xcode command-line tools and CMake. On Windows, use the MSVC Rust toolchain with the Visual Studio C++ build tools and CMake available in the build environment. The Linux build is exercised locally; the native macOS/Windows CI jobs provide additional build coverage when run.

```sh
node scripts/build.mjs --check
node scripts/build.mjs
./dist/hycli open
```

In PowerShell, the last command is `./dist/hycli.exe open`. The build script installs the locked dashboard dependencies, checks TypeScript and all translations, builds the dashboard, builds the Rust engine and desktop launcher and writes `dist/SHA256SUMS`. `--skip-dependencies` reuses an existing `dashboard/node_modules` directory; normal builds use `npm ci`.

To place the executable in a directory you already use on PATH:

```sh
node scripts/build.mjs --install-dir "$HOME/.local/bin"
hycli open
```

Windows example: `node scripts/build.mjs --install-dir "$env:LOCALAPPDATA\Hycli\bin"`. Add that directory to your user PATH or run the printed absolute executable path. Installation does not edit shell profiles or machine settings.

`CARGO_TARGET_DIR` and `CARGO_BUILD_TARGET` are respected. Keep the complete source checkout while building; the release executable itself can be moved afterwards. The debug executable reads dashboard assets from the build checkout.

## Start and reconnect

Open Hycli from its installed application icon, by running `hycli open`, or by running `hycli` without arguments. The launcher starts the background engine and opens the default browser. It prefers `127.0.0.1:4318`, falls back to a free loopback port if occupied, and reuses a responding instance for the same data directory. The terminal can close immediately. `hycli open --port 4320 --no-open` starts without a browser and prints the public address plus an `opening_file` path. Open that private HTML file within two minutes to authorize a browser window; run `hycli open --no-open` again if it expires. `hycli status` reports only the non-secret public address, not a browser sign-in credential.

Closing the browser keeps the engine running. Use **Settings → Quit Hycli** or `hycli stop` to stop it; active work must finish or be cancelled first. No system service or login startup is installed. Startup diagnostics are in `dashboard.log` in the data directory. The `dashboard-instance.json` file is only a discovery record: Hycli verifies a random instance ID over loopback before reusing or stopping it, and never kills a PID read from that file.

For development, `hycli dashboard --port 4320 --no-open` retains foreground operation and fails if its requested port is occupied. SIGINT and SIGTERM (Unix) shut it down. Use the same data directory for the foreground and desktop modes.

Automatic opening passes only an owner-private temporary file path to the operating system, never a reusable credential or a ticket-bearing URL. Each opening has an independent, single-use ticket. The file is removed after consumption, expiry or shutdown; an opener starting successfully does not consume it. If the operating system cannot establish owner-only file access, opening fails closed. Browser-session state is stored separately in the owner-private `dashboard-auth-v2` directory. The first upgrade invalidates the old launch-derived browser session once; reopen Hycli to reconnect. Accounts, settings and jobs are retained.

An open dashboard using the new browser session reconnects after a server restart. Preparation and descriptions can be retried from Activity. Completed results persist. A change interrupted after authorization may already have reached its website; its activity entry tells you to check the website before running it again. Hycli never replays that change automatically.

Data is stored under `HYCLI_DATA_DIR` when set, otherwise `XDG_DATA_HOME/hycli` when set, then the platform's user data directory: `~/.local/share/hycli` on Linux, `~/Library/Application Support/hycli` on macOS, and `%LOCALAPPDATA%\hycli` on Windows. An existing installation at the earlier `~/.local/share/hycli` location is retained. Use one data directory for the dashboard, CLI and MCP. Back it up while Hycli is stopped; it includes encrypted credentials and their local key fallback when the OS keyring is unavailable.

## Connections

Connect an AI provider from **AI connections**. The ChatGPT login method uses an installed Codex executable on PATH (`codex.exe` on Windows). On macOS, if PATH has no executable candidate, Hycli also tries the [documented app-bundled executable](https://learn.chatgpt.com/docs/reference/troubleshooting) at `/Applications/Codex.app/Contents/Resources/codex`. Finding an executable does not establish a signed-in account; use **AI connections** to check or complete ChatGPT sign-in. Other provider methods use your API key. A browser is needed to view the dashboard. Rendered HTML reads and the optional browser diagnostic need a discovered Chromium-based browser. `CHROME` can select its executable; Hycli does not download a browser automatically. Native cookie/storage import also supports Firefox profiles independently of rendering.

Website preparation begins with the AI selecting complete user workflows from website evidence and the optional purpose. This first judgment does not assess the machine’s available capabilities. Specialists then implement that plan, and a reviewer checks every prerequisite, required input and result. Ordered read checks can pass an earlier result’s identifier into the next operation; the broker retains the value and returns only its shape to the reviewer. Missing or unverified read steps keep the site marked for review.

For websites, Hycli inventories the current environment and gives the AI scoped tools to choose its next step. Browser cookies and origin storage can supply an existing session; a generated connection recipe references local keys without exposing their values to AI. An actual current-account response verifies identity. A supported SiteSpec can also declare a website API key or bearer credential; **Explore actions → Website API keys** saves that value locally. Preparation records whether useful reads passed their expected response format.

The browser companion remains an optional fallback for browsers whose protected sign-ins cannot be read locally. Companion archives and setup instructions are available in **Accounts**. Store publication/signing is separate from local unpacked or temporary installation.

### Verify a browser session from the CLI

`hycli accounts` still lists saved accounts. When automatic current-account discovery is insufficient, the CLI can inspect and verify an evidence-backed connection without an AI preparation loop:

```sh
hycli accounts inspect ACCOUNT_ID
hycli accounts observe ACCOUNT_ID https://website.example/current-context
hycli accounts verify ACCOUNT_ID --recipe connection.json
hycli accounts refresh ACCOUNT_ID
```

Inspection and observation return key names, request shapes and response types, never cookies, authentication headers or private response values. Known credential values are also redacted from metadata and recipe references. Identity fields that echo those values are rejected before account metadata is saved. Observation alone does not verify identity. The JSON connection recipe uses the same `identity_url`, local `headers` references, `identity` or `identity_html` mapping, and `evidence` fields used during preparation; it must not contain secret values. Verification requires a GET path already observed for this account's website, enforces an exact match to its saved credential origin (not sibling subdomains) and the read-only URL policy, and rejects identities also returned without authentication. Anonymous redirects, rate limits and server errors are inconclusive and never establish verification. Use an actual current-account response, not a public profile. A failed verification preserves the previously verified recipe. Refresh reuses that recipe with the same single-request verification and anonymous baseline; an expired or credential-echo identity is not reported as verified.

For a JSON current-account endpoint, `connection.json` can start with:

```json
{
  "identity_url": "https://website.example/current-context",
  "identity": {"user_id": "/id", "name": "/name"},
  "evidence": "observed_browser_request"
}
```

If the endpoint requires an authentication header, add a `headers` entry such as `{"header":"X-Csrf-Token","source":"cookie","key":"csrf","prefix":""}` using names returned by inspection. Header rules may reference `cookie`, `local_storage` or `captured_header` sources; copy key names, never their values. HTTP observation makes one bounded request and does not follow redirects or automatically start a browser. Browser observation (`--browser`) currently returns `browser_diagnostic_unsupported` before browser launch, network activity or credential updates. Page-level interception alone does not establish read-only, exact-origin enforcement for workers, subframes and WebSockets, so this mode intentionally fails closed. Observe a known exact-origin endpoint over HTTP instead. Diagnostics never automatically expand origin permissions. For useful one-off JSON and GraphQL reads without an installed action, use the separately gated `hycli accounts read ACCOUNT --request REQUEST.json` flow in [Account reads](ACCOUNT_READ.md).

## Reproducible verification

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo test --locked --features dashboard-test-support --test codex_launch
cargo build --locked --bin hycli
cargo build --locked --features dashboard-test-support --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

The `dashboard-test-support` feature is only for synthetic fixtures; do not enable it for production or package builds. Source archives include the gated fixture helper and test scripts for reproducibility; default native builds do not compile the helper, and native packages do not include fixture executables or session files. Fixture sessions are established by test-only ticket issuance followed by the normal bootstrap endpoint. Browser suites install that synthetic session to test protected UI behavior; they do not verify the private-file/native-browser handoff.

Install the test browser with `cd dashboard && npx playwright install chromium`, or set `BROWSER_BIN` to an installed Chromium executable. When a distribution launcher adds personal graphics/session flags, use the browser binary directly for isolated tests.

`HYCLI_TEST_DATA` can select a different parent directory for isolated test data and browser profiles; the default is `.cache/` inside the project.

The aggregate browser runner skips documentation screenshots so verification does not change reviewed source assets. Set `HYCLI_NO_SCREENSHOTS=0` to deliberately regenerate them from the synthetic fixtures. Direct browser suites generate screenshots by default; set `HYCLI_NO_SCREENSHOTS=1` to skip them. Failure diagnostics under `.cache/` remain available.

The browser tests run real Rust dashboard and MCP APIs against synthetic local website/provider servers. They cover exact approval, secret redaction, account resume, website-key setup and rotation, summary failure/retry, duplicate jobs, cancellation, management and server-session reconnection. Mutations stay in those fixtures. Live paid provider accounts and every external website are not prerequisites for the fixture suite.

An optional live check uses the already signed-in Codex connection and an explicitly supplied public website: `cargo run --example live_prepare -- https://your-public-website.example "The intended task"`. It prepares actions, runs one evidenced public read with complete inputs, and summarizes the result. It uses provider quota and performs no mutation.

## Shareable packages

```sh
node scripts/package.mjs
node scripts/package-desktop.mjs
node scripts/package.mjs --source
```

The first command packages the native binaries already built in `dist/`; the desktop command creates a DEB, DMG or Windows installer on its native OS; `--source` creates an allowlisted public source snapshot without Git history. Both write archives, checksums and file manifests under `dist/artifacts/`. Neither copies installed site definitions, accounts or browser profiles. See [RELEASING.md](RELEASING.md) for archive extraction and fresh-data smoke verification.

Native release packages contain `agent/AGENTS.md`, the byte-identical `agent/CLAUDE.md`, `skills/hycli/` and the [operation reference](SITESPEC.md). `hycli mcp` exposes preparation and execution together. Use `--sites-only` only when that restricted scope is intended.

Desktop packaging prerequisites: Linux needs `dpkg-deb` and `objdump`; macOS uses `sips`, `iconutil`, `codesign` and `hdiutil` from the system tools; Windows needs NSIS 3.11+ (`makensis` on PATH, or set `MAKENSIS`). macOS packages require macOS 13 or later. Set `MACOSX_DEPLOYMENT_TARGET=13.0` when compiling for the documented floor. The DEB's libc dependency is derived from the built ELF symbols instead of claiming compatibility with an older distribution.

Desktop-specific verification:

```sh
node scripts/smoke-desktop.mjs dist/hycli --browser
node scripts/smoke-installer.mjs dist/artifacts/hycli-0.1.1-linux-x64.deb
```

Use `.exe` for the binary on Windows and the corresponding `.dmg` or `*-setup.exe` artifact on macOS/Windows. `--browser` exercises the actual translated Quit action through Playwright; omit it for non-browser native checks. Tests cover concurrent launches, port conflicts, stale discovery records, shutdown authorization, launcher execution and saved settings across restarts. Installer checks inspect an extracted DEB, a mounted DMG or a silent per-user Windows installation. They do not replace testing the final signed installer on a clean user machine.

Windows release builds select the MSVC target explicitly and link the C runtime statically. The build checks both PE import tables and refuses a direct Visual C++ runtime DLL dependency. Packages target Windows 10 or newer; Windows system DLLs remain runtime dependencies. No separate Rust, Node.js or Visual C++ redistributable installation is needed for these release binaries.

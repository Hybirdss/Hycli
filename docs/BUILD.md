# Build and run Hycli

Hycli runs locally. The release executable includes the dashboard and website engine; Node.js and Rust are build tools, not runtime requirements.

Native packages still use the target operating system's libraries. Linux GNU builds require compatible glibc and libgcc runtimes; the Ubuntu 24.04 CI build targets a glibc 2.39 baseline. An archive built on another distribution can require a newer baseline. Check the package's build platform before installing; build from source for older distributions or musl-only systems such as Alpine. macOS and Windows packages must match the machine's operating system and architecture.

## Build from a checkout

Use Rust 1.98 or newer and Node.js 20.19+ or 22.12+. The lockfiles select the Rust and JavaScript dependencies.

The standard HTTP transport uses Rustls with AWS-LC. A native C/C++ compiler is required; CMake covers the TLS library's fallback build path. The locked dependencies use generated bindings and do not require Perl or libclang. On Debian/Ubuntu:

```sh
sudo apt-get install build-essential cmake pkg-config git
```

On macOS, install Xcode command-line tools and CMake. On Windows, use the MSVC Rust toolchain with the Visual Studio C++ build tools and CMake available in the build environment. The Linux build is exercised locally; the native macOS/Windows CI jobs provide additional build coverage when run.

```sh
node scripts/build.mjs --check
node scripts/build.mjs
./dist/hycli dashboard
```

In PowerShell, the last command is `./dist/hycli.exe dashboard`. The build script installs the locked dashboard dependencies, checks TypeScript and all translations, builds the dashboard, builds the Rust release executable and writes `dist/SHA256SUMS`. `--skip-dependencies` reuses an existing `dashboard/node_modules` directory; normal builds use `npm ci`.

To place the executable in a directory you already use on PATH:

```sh
node scripts/build.mjs --install-dir "$HOME/.local/bin"
hycli dashboard
```

Windows example: `node scripts/build.mjs --install-dir "$env:LOCALAPPDATA\Hycli\bin"`. Add that directory to your user PATH or run the printed absolute executable path. Installation does not edit shell profiles or machine settings.

`CARGO_TARGET_DIR` and `CARGO_BUILD_TARGET` are respected. Keep the complete source checkout while building; the release executable itself can be moved afterwards. The debug executable reads dashboard assets from the build checkout.

## Start and reconnect

The dashboard binds to `127.0.0.1:4318`. `hycli dashboard --port 4320 --no-open` prints a different local address without launching a browser. Keep that process running while using the dashboard. If a port is occupied, choose another port. If the same data directory already has a running dashboard, use that instance instead of starting a second writer.

An open dashboard reconnects after a server restart. Preparation and descriptions can be retried from Activity. Completed results persist. A change interrupted after authorization may already have reached its website; its activity entry tells you to check the website before running it again. Hycli never replays that change automatically.

Data is stored under `HYCLI_DATA_DIR` when set, otherwise `XDG_DATA_HOME/hycli` when set, then the platform's user data directory: `~/.local/share/hycli` on Linux, `~/Library/Application Support/hycli` on macOS, and `%LOCALAPPDATA%\hycli` on Windows. An existing installation at the earlier `~/.local/share/hycli` location is retained. Use one data directory for the dashboard, CLI and MCP. Back it up while Hycli is stopped; it includes encrypted credentials and their local key fallback when the OS keyring is unavailable.

## Connections

Connect an AI provider from **AI connections**. The ChatGPT login method requires an installed `codex` executable on PATH; other provider methods use your API key. A browser is needed to view the dashboard. Rendered HTML reads and the optional browser diagnostic need a discovered Chromium-based browser. `CHROME` can select its executable; Hycli does not download a browser automatically. Native cookie/storage import also supports Firefox profiles independently of rendering.

Website preparation begins with the AI selecting complete user workflows from website evidence and the optional purpose. This first judgment does not assess the machine’s available capabilities. Specialists then implement that plan, and a reviewer checks every prerequisite, required input and result. Ordered read checks can pass an earlier result’s identifier into the next operation; the broker retains the value and returns only its shape to the reviewer. Missing or unverified read steps keep the site marked for review.

For websites, Hycli inventories the current environment and gives the AI scoped tools to choose its next step. Browser cookies and origin storage can supply an existing session; a generated connection recipe references local keys without exposing their values to AI. An actual current-account response verifies identity. A supported SiteSpec can also declare a website API key or bearer credential; **Explore actions → Website API keys** saves that value locally. Preparation records whether useful reads passed their expected response format.

The browser companion remains an optional fallback for browsers whose protected sign-ins cannot be read locally. Companion archives and setup instructions are available in **Accounts**. Store publication/signing is separate from local unpacked or temporary installation.

## Reproducible verification

```sh
npm --prefix dashboard ci
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

Install the test browser with `cd dashboard && npx playwright install chromium`, or set `BROWSER_BIN` to an installed Chromium executable. When a distribution launcher adds personal graphics/session flags, use the browser binary directly for isolated tests.

`HYCLI_TEST_DATA` can select a different parent directory for isolated test data and browser profiles; the default is `.cache/` inside the project.

The browser tests run real Rust dashboard and MCP APIs against synthetic local website/provider servers. They cover exact approval, secret redaction, account resume, website-key setup and rotation, summary failure/retry, duplicate jobs, cancellation, management and server-session reconnection. Mutations stay in those fixtures. Live paid provider accounts and every external website are not prerequisites for the fixture suite.

An optional live check uses the already signed-in Codex connection and an explicitly supplied public website: `cargo run --example live_prepare -- https://your-public-website.example "The intended task"`. It prepares actions, runs one evidenced public read with complete inputs, and summarizes the result. It uses provider quota and performs no mutation.

## Shareable packages

```sh
node scripts/package.mjs
node scripts/package.mjs --source
```

The first command packages the native executable already built in `dist/`; the second creates an allowlisted public source snapshot without Git history. Both write archives, checksums and file manifests under `dist/artifacts/`. Neither copies installed site definitions, accounts or browser profiles. See [RELEASING.md](RELEASING.md) for archive extraction and fresh-data smoke verification.

Native release packages contain `agent/AGENTS.md`, the byte-identical `agent/CLAUDE.md`, `skills/hycli/` and the [operation reference](SITESPEC.md). `hycli mcp` exposes preparation and execution together. Use `--sites-only` only when that restricted scope is intended.

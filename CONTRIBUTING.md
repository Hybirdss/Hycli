# Contributing to Hycli

Hycli turns a website task into useful CLI and MCP actions. A good change makes that path clearer, more reliable or easier to verify.

## Get running

Install the prerequisites in [BUILD.md](docs/BUILD.md), then:

```sh
npm --prefix dashboard ci
npm --prefix dashboard run build
cargo build --locked --bin hycli
./target/debug/hycli dashboard
```

If `CARGO_TARGET_DIR` is set, use the executable under that directory. Start with a fresh `HYCLI_DATA_DIR` when checking changes to accounts or preparation.

## Find your way around

| Area | Location |
| --- | --- |
| Website preparation and available tools | `src/preparation.rs`, `src/agent_tools.rs` |
| Shared CLI, MCP and dashboard execution | `src/runtime.rs`, `src/policy.rs`, `src/net.rs` |
| Website definitions and account connections | `src/spec.rs`, `src/session_recipe.rs` |
| Local dashboard | `dashboard/src/` |
| Browser companion | `browser-companion/` |
| Portable agent instructions | `agent/`, `skills/hycli/` |
| Build, packaging and integration checks | `scripts/`, `dashboard/*-e2e.mjs` |

Read [AGENTS.md](AGENTS.md) before changing the execution contract and [SITESPEC.md](docs/SITESPEC.md) before adding an operation type.

## Make a focused change

Describe the concrete trigger and expected result in an issue or pull request. Keep unrelated changes separate. Include a screenshot for visible dashboard changes and preserve keyboard access, narrow layouts and right-to-left languages.

The English dashboard locale is the reference for keys. Update all 20 locales when adding UI text. Keep root `AGENTS.md` and `CLAUDE.md` identical, and do the same for the guides in `agent/`. Update README translations when installation or product behavior changes.

## Verify the behavior

The full local check sequence is:

```sh
npm --prefix dashboard run check
npm --prefix dashboard run build
cargo fmt --all --check
cargo test --all-targets --locked
cargo build --locked --bin hycli --example dashboard_fixture
node scripts/test-e2e.mjs --full
```

Browser setup and platform prerequisites are in [BUILD.md](docs/BUILD.md). Use synthetic local services for operations that change data. Do not create, edit or delete real website content merely to test a contribution. A platform or website is verified only when the relevant check has actually run.

For a pull request, include the problem, the resulting behavior and the checks you ran. Flag any checks you could not run. Keep account data, credentials, browser profiles, private logs and generated site definitions out of commits and attachments.

Maintainers may merge an original contribution after independent review and complete verification. Separate improvements belong in follow-up pull requests. Known correctness or security defects, failing checks, and required checks that have not run must be resolved before merging. Preserve original authorship and reporter credit; adaptations should retain verified `Co-authored-by:` metadata and contributor attribution.

## Package a release

Follow [RELEASING.md](docs/RELEASING.md). Source and native archives use explicit file lists. A new root document or asset may need to be added to `scripts/package.mjs`; checking `.gitignore` alone does not verify a release archive.

## Contributors

- [@ytw418](https://github.com/ytw418) (윤성준) — AI provider refusal classification (`ai_refused`), partial action preservation across preparation rounds, and `self_qa` prompt hardening.
- [@jjigmeok-dev](https://github.com/jjigmeok-dev) — feedback/community entry points and environment-aware issue templates ([#15](https://github.com/Hybirdss/Hycli/pull/15)), plus MCP bundles and registry metadata ([#16](https://github.com/Hybirdss/Hycli/pull/16)).
- [@seungwonme](https://github.com/seungwonme) — reported OAuth API token refresh continuity ([#41](https://github.com/Hybirdss/Hycli/issues/41)), ordinary API denial misclassification ([#42](https://github.com/Hybirdss/Hycli/issues/42)), pagination continuation, browser-session import and aggregate browser-suite failures ([#21](https://github.com/Hybirdss/Hycli/issues/21), [#22](https://github.com/Hybirdss/Hycli/issues/22), [#24](https://github.com/Hybirdss/Hycli/issues/24)); contributed bounded session diagnostics, identity verification and credential-echo regression tests ([#26](https://github.com/Hybirdss/Hycli/pull/26)).
- [@kayboram](https://github.com/kayboram) contributed browser-discovery filtering and response-contract regression coverage in [#53](https://github.com/Hybirdss/Hycli/pull/53), whose review prompted the account-continuity correction in [#55](https://github.com/Hybirdss/Hycli/pull/55).
- [@mechurisr](https://github.com/mechurisr) reported the dashboard launch-credential exposure in [#54](https://github.com/Hybirdss/Hycli/issues/54).
- [@sesol-lumen](https://github.com/sesol-lumen) contributed the preflight Bearer-header compatibility proposal in [#58](https://github.com/Hybirdss/Hycli/pull/58) and the cross-account preference-cookie output correction in [#60](https://github.com/Hybirdss/Hycli/pull/60).

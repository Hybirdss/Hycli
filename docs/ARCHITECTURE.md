# Hycli architecture

The executable hosts the local dashboard, CLI and MCP server. All three use the same runtime, credential scope, input validation and approval records.

| Area | Main files |
| --- | --- |
| Website preparation and evidence | `src/preparation.rs`, `src/agent_tools.rs`, `src/discovery.rs` |
| Portable operation definitions and HTML extraction | `src/spec.rs`, `src/html.rs` |
| Execution, request policy and HTTP | `src/runtime.rs`, `src/policy.rs`, `src/net.rs` |
| Browser discovery, stored sessions and rendered reads | `src/environment.rs`, `src/browser_discovery.rs`, `src/browser_storage.rs`, `src/browser_session.rs`, `src/session_recipe.rs` |
| AI providers and managed sign-in | `src/ai/` |
| Dashboard and client | `src/dashboard.rs`, `dashboard/src/` |
| CLI and MCP | `src/main.rs`, `src/cli_actions.rs`, `src/guarded_mcp.rs` |
| Reproducible synthetic services | `examples/dashboard_fixture.rs`, `tests/cli.rs`, `dashboard/*-e2e.mjs` |

Read [BUILD.md](BUILD.md) for prerequisites, commands and release verification. End-user agent guidance lives in [agent/AGENTS.md](../agent/AGENTS.md); the portable skill is [skills/hycli/SKILL.md](../skills/hycli/SKILL.md).

The frontend build produces `web/`. Release Rust builds embed those assets. Debug builds use the checkout assets, so build the frontend before running a debug dashboard.

Tests use isolated synthetic websites and provider responses. Public screenshots come from the running fixture dashboard. A passing fixture does not establish that a paid provider, a particular live account or an unexecuted native OS was tested.

Keep developer run logs, generated site definitions, credentials, browser profiles and private connection data outside public source packages. Packaging scripts use explicit file lists. Source archives omit Git history, so a private development history is not accidentally published with the source tree.

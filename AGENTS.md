# Contributing to Hycli

Hycli turns a user's website task into usable local CLI and MCP actions. Favor an agent that observes the environment, chooses an available capability, verifies results, and adapts from failures. A site-specific patch or a developer's installed browser path is not a portable implementation.

## Architecture

- `src/preparation.rs` and `src/agent_tools.rs` coordinate evidence-led website preparation.
- `src/runtime.rs`, `src/policy.rs` and `src/net.rs` are the shared execution boundary for CLI, MCP and dashboard actions.
- `src/spec.rs` defines the portable operation contract. `src/session_recipe.rs` binds observed site identity to local credential sources.
- `src/environment.rs` and browser adapters resolve local capabilities; machine paths must not enter generated definitions.
- `src/main.rs` and `src/guarded_mcp.rs` expose CLI and MCP. `src/dashboard.rs` serves the React app in `dashboard/src`.

Make failure actionable. Do not report a login page, a successful homepage fetch, a produced schema, or an unexecuted plan as a working integration. Preserve installed definitions and account data while preparing replacements. Keep UI and command behavior understandable to a new user without this repository's history.

## Work and verification

Preserve unrelated work. Use the repository's lockfiles and keep temporary fixtures under `.cache/` or an explicitly selected external test directory. Tests must not read a developer's profile or depend on a globally configured provider.

The standard verification flow is documented in `docs/BUILD.md`: build and check the frontend, run `cargo test --all-targets --locked`, build the CLI and `dashboard_fixture`, then run `node scripts/test-e2e.mjs --full`. Run checks that exercise the changed behavior, including actual CLI/MCP calls when their contract changes. Test mutations only against synthetic local services. Label native platforms and live website behavior as unverified until they have actually run.

## Distribution and documentation

Ship the executable, public instructions, licenses and intentional assets. Never ship installed site definitions, accounts, credentials, private traces, local databases, user browser profiles, tool state, or unreviewed debug executables. Check the actual source archive and binary package contents, not just `.gitignore`.

Public screenshots must capture the running app with isolated demonstration data and no real target or account. Keep product behavior truthful: explain a missing capability and its next step instead of promising unsupported execution.

`agent/AGENTS.md` is the canonical end-user agent guidance embedded in the dashboard. Keep `agent/CLAUDE.md` byte-identical to it. Keep root `CLAUDE.md` byte-identical to this file. `skills/hycli/SKILL.md` is the portable end-user skill; validate its commands against the actual CLI and MCP surfaces. Synchronize README source and translated entry points when behavior or installation changes.

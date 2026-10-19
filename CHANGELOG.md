# Changelog

## Unreleased

Automate any website, with changes only where you allow them.

### Safety

- Every website starts read-only. Its change actions are hidden from MCP and refused by the CLI with `writes_disabled` until you turn on **Allow changes** for that website in the dashboard. Turning it off cancels pending approvals.
- The dashboard grants its session only to the address `hycli dashboard` opens. A plain local request can no longer obtain the session that approves changes. The session persists in the private data directory, so an open dashboard still reconnects after a restart.
- Credentials resolved by `derived` authentication or secret-classified `preflight` steps (`cookie:*`, `header:*`, or token/CSRF fields) are automatically registered for output redaction (`[private]`), while non-secret page identifiers (`buildId`, workspace IDs, cursors) remain intact and can be interpolated into URL paths, query strings, headers and bodies with strict path-segment URL encoding and origin pinning.

### Deterministic Recon & SiteSpec v2

- `hycli recon <URL>` and MCP `hycli_recon` deterministically inspect SSR state payloads (`__NEXT_DATA__`, `__NUXT_DATA__`, Remix, Apollo, JSON-LD), HTML `<form>` structures, and up to 24 JavaScript bundles (prioritizing app/page chunks over vendor/runtime bundles and fetching cross-site CDN bundles anonymously without cookies), and can emit a starter `SiteSpec v2` draft without requiring a built-in LLM key.
- `SiteSpec v2` adds `derived` authentication (`cookie:<name>`, `storage:<key>`, `header:<name>`), `preflight` step extraction (`json:/pointer`, `ssr:<key>#<pointer>`, `css:selector@attr`, `header:name`, `cookie:name`), `body.json_template`, and response preprocessing (`strip_prefix`, `ssr_json`, `extract_pointer`).
- Dashboard preparation (`Runtime::prepare`) has full parity with coding-agent workflows: Round 0 automatically reads prioritized `<script src>` bundles without stripping JS syntax, surfaces `ssr_keys` and structural `<form>` descriptors, exposes `web.recon` to preparation workers, prompts and merges `SiteSpec v2` definitions (`derived` auth, `preflight` steps, `{{step.*}}` path/query/header/body templates, and `ssr_json`), and matches parameterized routes during verification.
- Structured adversarial self-interrogation (`self_qa`) runs across every preparation phase (`understand`, `explore`, `organize`, `check`, and `review`), forcing agents to sharpen raw data surfaces, attack failed premises instead of repeating requests, exhaust the design space (`ssr_json`, `preflight`, `derived` auth, `body.json_template`) before declaring capability gaps, and persist a secret-redacted `self_qa` audit trail across rounds. Earlier answers stay in that audit trail and are not sent back to the model, which an AI provider can refuse as reasoning extraction.
- Same-site (`eTLD+1`) subdomain API hosts share verified browser cookies and origin storage within the same site integration, and Chromium observation captures preflight `fetch`/`XHR` headers and bodies via CDP `Fetch.requestPaused` without executing state-changing requests.
- Resilient WAF/CAPTCHA classification and execution: normal `200 OK` JSON/HTML responses and application-level `403` JSON errors behind CDNs no longer trigger false-positive bot lockouts; domain circuit breakers auto-recover after cooldown or `resume_auth()`; HTTP requests emit coherent browser Fetch Metadata (`Sec-Fetch-*`, `Accept`, same-origin `Referer`/`Origin`) with micro-jittered rate limiting; read-only `GET` requests automatically escalate from static HTTP to rendered Chromium (`fetch_rendered`) on JS/WAF challenges; preflight steps inspect all `Set-Cookie` headers; `storage:<key>#<pointer>` unwraps nested JSON-in-JSON strings; and SSR extraction captures `data-target` / `data-island` / `data-ssr` embedded JSON islands.

### Checks without AI

- `hycli check [SITE]` and MCP `hycli_check` replay the reads that passed during preparation, including chained reads, after confirming the saved account. Verdicts separate `signed_out` from `site_changed`, `blocked` and `unreachable`, and the exit status follows the verdict. It replaces `hycli health`, which remains an alias.

### Complete website CLIs

- Claude API connection failures now distinguish billing, permissions, unavailable models and invalid requests without exposing provider response data. Connection setup trims surrounding whitespace from pasted keys and model IDs, rejects malformed keys locally, and preserves the previous connection if validation fails.
- Preparation plans complete user workflows, reviews every step, and verifies chained reads before installing.
- After the workflows, workers continue through documented API operations that have no action yet. The job reports `coverage` with any remaining operations.
- Anthropic connections now default to `claude-opus-4-8` to avoid false-positive safety refusals on `claude-opus-5-5`, and a request the AI provider declines is reported as `ai_refused` instead of `ai_unavailable`; retrying the same request does not help.
- If every worker in a later round fails, preparation checks and keeps the actions earlier rounds found instead of discarding them.

### Native desktop & installers

- Open Hycli from a native app icon or `hycli open` without keeping a terminal or manually starting a server. Reuse running instances and select a free local port when necessary.
- Quit from Settings in all 20 languages, or use `hycli status` / `hycli stop`. Preserve active work and saved data.
- Build per-user Windows installers, macOS app/DMG packages and Linux DEBs, plus portable archives with a Linux menu installer, for x64 and ARM64.
- Verify background lifecycle and actual installer payloads in each native CI job. Support optional Windows signing and macOS Developer ID signing/notarization.

### Feedback and community

- `hycli feedback` requests a browser launch for a generic GitHub issue with version and platform details. It always returns a structured URL and launch-request status, even without a browser launcher; opening the URL contacts GitHub, but does not submit an issue.
- The empty dashboard and translated README entry points link to Issues for help. The bug template asks for version, operating system and installation path; the feature template includes optional version and operating system fields.

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

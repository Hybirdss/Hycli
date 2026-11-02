# Synthetic cookie permission proof

This separate GitHub Actions job measures native cookie API visibility. It does
not load or change the production companion, transfer cookies to Hycli, or prove
that a real login works. No website or account is contacted.

## Supported target and safety boundary

The target is GitHub's `windows-2022` runner with the Chromium version installed by
the existing dashboard Playwright lockfile. Playwright documents Windows Server
2019+ support and headless extension loading with bundled Chromium, a persistent
context and `channel: 'chromium'`:

- https://playwright.dev/docs/intro#system-requirements
- https://playwright.dev/docs/chrome-extensions
- https://github.com/actions/runner-images/blob/main/images/windows/Windows2022-Readme.md

The test explicitly enables `chromiumSandbox`. It does not change machine/browser
policies, AppArmor, sysctl, security permissions or installed browser profiles.
If loading or sandbox initialization is restricted, the job fails rather than
weakening those settings. It cannot select a different executable through an
environment variable. The native runner entry point refuses non-Windows or
non-GitHub-Actions execution. Do not override this guard to work around a managed
desktop policy.

The existing `extension-e2e.mjs` integration test is separate and is not reused:
this proof uses its own minimal synthetic manifest and service worker, not the
production extension or dashboard fixture. It adds no product CLI or MCP tool.

## Actual scope

Three fresh profiles use required manifest grants: none, exact HTTPS www, and
exact HTTPS apex plus www. There are no wildcard-host grants. Synthetic cookies
cover www host-only, apex host-only, parent domain, www domain, unrelated domain,
and a separately keyed partitioned parent cookie. All names and values are fake.

The test first verifies that all fixtures exist, then checks native `getAll`
results with no filter, URL filter, explicit normal store and partition key.
Negative controls reject unrelated-origin access, apex host-only URL leakage,
missing permitted fixtures and partition leakage into default queries. The
www-only parent-domain result is recorded, not assumed. Both possible observations
remain distinguishable in the report with the actual browser version.

Required manifest grants are NOT a test of optional permission prompts, denial,
revocation, private stores, Firefox containers/FPI or branded browsers. Those
remain unverified. A green result is a Chromium normal-store API proof only,
not an all-browser pass and not a universal requirement for apex permission.

Official semantics:
- https://developer.chrome.com/docs/extensions/reference/api/cookies
- https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/cookies

## Run and inspect

Use the `Synthetic cookie permission matrix` workflow on a reviewed repository
revision. It installs dependencies using `npm ci`, then the matching bundled
Chromium. The pure contract tests can run without a browser:

    cd dashboard
    node --test cookie-permission-contract.test.mjs

The native entry point is run only by the dedicated job:

    node cookie-permission-e2e.mjs

The artifact contains only `test-results/cookie-permission-report.json`, including
started/completed cases and any error. Browser profiles are created in a unique
owned temporary directory and removed after context shutdown. Profiles, cookie
values, credentials, and browser traces are not uploaded. A setup failure with no
report is still a failed CI step, never a passing native test.

Until that workflow has actually run, the native matrix is unverified. Passing
pure contract tests validates the report checker, not browser behavior.

## Packaged companion popup gate

`dashboard/companion-consent-native.mjs` runs after the ordinary Rust build in the Windows x64 verification job. It downloads the actual companion archive from an isolated Rust fixture and checks the production JavaScript and parser/license bytes before loading it. Only the owned test manifest's required host grants are extended with exact synthetic fixture hosts, following the raw API matrix above. It uses the bundled Chromium with `chromiumSandbox: true`, a fresh disposable normal profile, and no user browser path or account.

The test calls the supported `chrome.action.openPopup()` API and drives the actual browser-action popup using CDP input events. It does not open a fake popup as a tab or replace `tabs.query`. The active website must remain the paired origin. A missing or uncontrollable action-popup target fails with a concrete capability error; it must not fall back to disabled sandboxing, browser policy changes or managed-cloud extension loading.

The gate checks displayed finite scopes, native-cookie import with both existing grants, apex host-only exclusion, empty native visibility preserving saved state, a changed active website being rejected, cookie-only ancestor grants not recording identity/forwarding traffic, and delayed forbidden responses preserving a replaced connection. A separate real Rust fixture control makes its account endpoint return HTTP 401, verifying account expiry is based on server rejection rather than empty cookie visibility.

This does **not** automate accepting or denying a new optional host-permission prompt. The browser already holds only the synthetic required grants; required grants cannot be revoked with `permissions.remove`. Native optional-prompt consent and revocation, private stores, Firefox and real accounts remain unverified. The source-level VM regressions separately cover denied/revoked permissions without claiming native-browser coverage.

Only a sanitized JSON result is uploaded. The profile, pairing/session data, fixture state and downloaded archive remain in the invocation-owned temporary directory and are removed on exit. The Windows gate is designed but must not be reported as passed until that exact commit executes successfully in CI.

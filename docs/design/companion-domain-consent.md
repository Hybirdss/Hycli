# Bounded parent-domain cookie consent

## Why and evidence

The companion previously requested only the paired origin before asking the browser for cookies. Native cookie visibility is browser- and permission-dependent; a successful empty query does not establish logout. This change addresses parent-domain visibility without assuming that every browser requires the same additional grant.

History consulted: `git log --follow -- browser-companion/popup.js` reaches the initial release. It contains no specific rationale for excluding parent grants. The existing comment explicitly requires a direct user gesture for permission requests; that constraint is preserved.

Browser contracts:
- [Chrome Cookies API](https://developer.chrome.com/docs/extensions/reference/api/cookies): cookie reads require host permissions and partitioned cookies require explicit partition selection.
- [Mozilla Cookies API](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/API/cookies): first-party isolation and storage partitions qualify cookie identity.
- [Chrome match patterns](https://developer.chrome.com/docs/extensions/develop/concepts/match-patterns) and [Mozilla match patterns](https://developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Match_patterns): host permissions are not equivalent to an application execution origin; Firefox does not support ports in patterns.

No external observability, exception tracker, or analytics evidence was available for this behavior. No conclusion about a particular live account follows from synthetic tests.

## How

1. Pairing fixes the application origin. An offline pinned Public Suffix List parser computes its registrable boundary, including private suffixes. For deeper names, the finite list includes intermediate parent hosts, stopping at that boundary. No hostname wildcard is generated. Public suffixes, IPs, local names, unknown suffixes and trailing-dot names do not generate extra hosts.
2. The popup displays the actual requested match patterns before the existing Connect gesture. Firefox patterns omit unsupported ports; the paired origin remains port-specific. Optional host permissions are granted only through the browser's normal consent UI.
3. Both ordinary and partition-query results pass through one filter. Host-only cookies must match the exact selected host. Domain cookies must apply to it and remain within its registrable boundary. Store, first-party isolation, secure transport, cookie path metadata, expiry and partition context remain separate properties. The partition key uses a schemeful registrable site, not the selected origin/port. Cookie paths are retained for the runtime's existing request-path checks.
4. The selected active tab, cookie store and permissions are checked before connection. A permission denial, changed context, failed read or empty result does not connect or overwrite a saved account. Refresh rechecks the connection and grants; empty visibility leaves the existing account unchanged. Server authentication validation still decides whether that account works.
5. Cookie-only parent grants do not authorize observing parent or sibling traffic. Request shapes are observed only for an existing exact-origin connection in its matching tab/store/private context. They are checked again before forwarding. Initial identity is therefore unknown until the connected site supplies identity evidence; no parent identity is substituted.

## Preserve / change / avoid / risk

Preserve the exact application origin, normal user-gesture consent, private/store isolation, runtime destination policy and existing account selection. Change only cookie visibility planning, filtering and dependent observation/refresh guards. Avoid wildcard host grants, pre-consent reads, silent account selection, public-suffix grants, remote parser downloads and treating missing cookies as proof of logout.

The key safety fact is that greater browser cookie visibility does not expand imported host-only cookies or application observation/execution destinations. Executable tests exercise the actual shipped popup, background and shared helper. Native browser permission semantics still require native tests; mocked extension APIs do not prove a browser's consent dialog or cookie visibility.

## Verification

Run `node --test browser-companion/site-scope.test.cjs` for finite-scope, PSL, IDNA, ports, tenant, cookie, popup gesture, denial, revocation, context-change and observation cases. The normal native workflow runs it alongside registry/fixture tests. Run `cargo test --lib companion::archive_tests::` to inspect both packaged extension archives, including parser and license. Run the existing real extension E2E and the controlled native cookie visibility matrix before merging. The synthetic tests must use isolated profiles and must not read a user's existing cookies.

The parser is bundled unmodified from tldts 7.4.18, MIT licensed. Its upstream source, package integrity and retained license are documented beside the vendor asset. Updating the PSL is a dependency update, not a runtime network operation.

Cookie permissions omit ports on both engines because cookies are host-oriented. The displayed permission list is the actual finite scheme/host grant, while the paired execution origin retains its port. The native Windows harness uses a nondefault fixture port for import. Separately, the source-level VM regression asserts that different-port traffic is not admitted by the observation boundary.

# Read one page with a verified account

`hycli accounts read ACCOUNT --request REQUEST.json` reads an evidenced JSON endpoint without installing a SiteSpec. Normal reads within this verified scope do not prompt for approval. This command cannot enable writes, change accounts, follow redirects, run a browser, execute a preflight, or automatically fetch all pages. Existing website write permissions remain unchanged. There is no corresponding new MCP or dashboard tool.

The account must already be connected and have a current private identity proof, obtained through the existing account verification flow. An imported HAR is request evidence, not an identity proof or an authorization grant. Hycli always obtains credentials from the current account vault and ignores capture cookies and authentication values. You do not need to export secrets.

## Known JSON GET

After verifying the account, observe the known endpoint once:

```sh
hycli accounts observe ACCOUNT https://website.example/api/items
```

Copy its `read_evidence` reference into a manually authored request file:

```json
{
  "version": 1,
  "request": {
    "method": "GET",
    "url": "https://website.example/api/items"
  },
  "evidence": {"observation": "ro_REFERENCE_FROM_OBSERVE"},
  "pagination": {"query": "pageToken", "next": "/next"}
}
```

Omit `pagination` when the endpoint does not use a supported opaque cursor. The observation reference binds the exact request to this account's current verified credential snapshot and lasts at most 24 hours. A reconnect, credential change, or new identity verification requires fresh observation. Old shape-only observations do not establish an exact request contract.

Alternatively, `"evidence": {"har": {"path": "capture.har", "entry": 0}}` selects the exact request from a local HAR. Paths are relative to REQUEST.json; entry indexes are zero-based. The URL, method, body, and any supported fixed client selector must match exactly. Capture query fields must agree with the URL. Captured authentication is not imported or used for authorization. HAR files may be sanitized to remove it. A capture is not signed authority: Hycli independently checks final request semantics and the current account proof.

GET plus success or observation alone does not prove a server has no effects. This interface applies the same practical evidenced-read policy as installed actions: known JSON endpoint provenance, explicit bounded read intent, current account authority, and no detected change or ambiguous protocol semantics. It does not promise that arbitrary remote server code is pure.

## GraphQL

Only a single query is supported. Mutations, subscriptions, batches, conflicting operation names, duplicate fields, unrecognized envelope fields, ambiguous selectors, and nested change intents are rejected before target dispatch.

```json
{
  "version": 1,
  "request": {
    "method": "POST",
    "url": "https://website.example/graphql",
    "json": {
      "query": "query Items($after: String) { items(after: $after) { id } pageInfo { endCursor } }",
      "operationName": "Items",
      "variables": {}
    }
  },
  "evidence": {"har": {"path": "capture.har", "entry": 3}},
  "pagination": {"variable": "after", "next": "/data/pageInfo/endCursor"}
}
```

For GET, put the URL-encoded GraphQL envelope in the URL and omit the body. For form POST, use `form` instead of `json`, with string values and JSON-encoded `variables`/`extensions`. POST URLs cannot also carry query parameters. Arbitrary REST POST is not supported.

A full document with an APQ extension must match the extension's SHA256 hash. No automatic fallback registers a query or resends a different body.

## Documentless persisted queries

A bare operation ID is unknown, even when a previous response succeeded. Add `persisted` when you have a matching APQ query document or verified first-party executable Relay evidence:

```json
{
  "version": 1,
  "request": {
    "method": "POST",
    "url": "https://website.example/graphql",
    "json": {
      "operationName": "Items",
      "extensions": {"persistedQuery": {"version": 1, "sha256Hash": "EXACT_SHA256_OF_THE_QUERY_DOCUMENT"}},
      "variables": {}
    }
  },
  "evidence": {"har": {"path": "capture.har", "entry": 3}},
  "persisted": {
    "format": "apollo",
    "evidence": {"path": "operations.json"}
  }
}
```

APQ evidence can be a local JSON artifact (`"evidence": {"path": "operations.json"}`) or an artifact response selected from a local HAR (`"evidence": {"har": {"path": "capture.har", "entry": 1}}`). Paths are relative to REQUEST.json. No public manifest, preload tag, source fetch, or authentication export is required: authority comes from the exact APQ wire hash matching the parsed query document, not the file's provenance. Files are capped at 1 MiB.

Opaque Relay IDs do not have this hash guarantee. Their artifact HAR entry must describe a successful exact-origin, query-free JavaScript GET with response content. Hycli fetches root HTML and the linked script without credentials or redirects, then requires exact bytes. Roots/artifacts are capped at 1 MiB, with at most 256 candidate references. This verifies request evidence, never refreshes or invalidates the current login.

Supported formats and trust limits:

- `apollo`: Apollo v1 persisted-query manifest with exact operation ID/client selector and consistent query body, name, and type. This JSON evidence is supported only for an actual APQ v1 `extensions.persistedQuery` request whose SHA256 matches the exact full query document. Set request `apollo_client_name` only for the fixed `apollographql-client-name` selector in the observed request.
- `relay`: Relay JSON ID-to-document map, also supported only as the full-document source for an actual hash-bound APQ envelope. A generic JSON fetch preload alone cannot authorize an opaque registered ID.
- `relay-artifact`: an opaque `doc_id`, `queryId`, or `id` can be checked against a narrowly parsed static ConcreteRequest with `kind: "Request"`, exact `params.id`, `params.name`, `params.operationKind: "query"`, and `params.text: null`. The source must be an actual first-party executable `script[src]` in root HTML. Module preload alone, `nomodule`, and integrity-constrained edges are unsupported. Every enforced response/meta CSP must permit the exact script through a supported `script-src-elem`, `script-src`, or `default-src` policy (`'self'` or exact same-origin source). Blocking or complex policies fail closed rather than being browser-emulated. Report-only CSP is not enforced. Supported generated static forms are parsed, never evaluated; dynamic calls, computed properties, spreads, reassignment, and other unsupported JavaScript fail closed.

There is no caller-controlled `apq` or effect flag. APQ authority comes from the actual request envelope and verified document hash, not an ID merely shaped like a hash. Opaque Apollo IDs and opaque Relay JSON mappings are not currently supported. Unverified opaque artifacts, changed executable source, unsupported deployment semantics, mismatched APQ hashes, and conflicting mappings produce a request-specific `unsupported_evidence` error before target dispatch. Supply matching supported evidence or the observed full query document; do not log out or reconnect merely to repair request evidence. Normal verified reads remain available.

Source formats: [Apollo](https://www.apollographql.com/docs/graphos/platform/security/persisted-queries), [Relay persisted queries](https://relay.dev/docs/guides/persisted-queries/), [Relay ConcreteRequest](https://github.com/facebook/relay/blob/main/packages/relay-runtime/util/RelayConcreteNode.js). Unknown formats and conflicting mappings remain unsupported; asking for blanket approval cannot substitute for missing evidence.

## Results and continuation

A successful response contains `account_id`, `status`, `body`, and `next_cursor`. Responses are capped at 2 MiB. Sign-in values are redacted from both keys and values. Redirects, expired sessions, rate limits, non-JSON responses, and GraphQL `errors` are errors, not successful data exports. Error diagnostics do not include raw response bodies or credentials.

```sh
hycli accounts read ACCOUNT --request REQUEST.json
hycli accounts read ACCOUNT --request REQUEST.json --cursor hc_RETURNED_HANDLE
```

Each invocation fetches one page. Handles expire after 15 minutes and bind the complete request/evidence digest, current private identity proof, account and credential snapshot. Do not edit the request file between pages. Raw cursors stay private, including echoes in response bodies; no global token-name exception is introduced. Supported cursors are 8–4096 bytes, non-URL strings, never credentials. Only a declared REST GET query parameter or GraphQL variable can receive a continuation. Short numeric page indexes and URL continuations are not supported.

The shared transport can perform one bounded retry after a connection/timeout failure or an unqualified HTTP 503. It rechecks current account authority after waits, before every retry/dispatch, and before publishing results or new cursors. It honors cooldowns and never retries GraphQL errors, redirects, or write operations through this surface.

Request files are limited to 128 KiB, HAR files to 8 MiB, and source artifacts to 1 MiB. The existing private Store is reused; OS keyring availability determines its protection. This feature does not change account security settings or promise encryption on every platform.

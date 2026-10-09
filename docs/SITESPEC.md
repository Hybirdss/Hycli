# Portable website actions

Hycli normally prepares this definition itself from the user's URL and intent. This reference is for agents and contributors that need to inspect or extend an integration. A definition describes evidence-backed operations; it contains no executable scripts, credentials or machine paths.

To add missing functionality without authoring a schema, use `hycli request SITE "Add draft editing and publishing with full post content"` (alias: `extend`). MCP exposes the same operation as `hycli_request` with `site` and `instruction`. The job reports `added_actions`, `available_actions` and `capability_gaps`; existing installed actions are preserved.

Validate with `hycli spec validate FILE`, install with `hycli spec install FILE`, and inspect the resulting interface with `hycli describe SITE ACTION` or `hycli SITE ACTION --help`. Use an isolated synthetic server for mutation tests.

## Requests

The root has `spec_version: 1`, `site` (`name`, `title`, `base_url`, optional `source_url`), optional `defaults.headers`, `auth`, and `operations`. Each operation has a unique `name`, `desc`, `method`, `/path`, `effect`, and `evidence`. `effect` is `read`, `write`, or `unknown`; an agent's declaration never overrides the runtime's classification.

| Field | Meaning |
| --- | --- |
| `params` | Named `{variables}` in the path, escaped as individual segments. |
| `query` | URL query parameters. |
| `body.json` | Fields serialized in a JSON object. |
| `body.form` | Fields encoded as `application/x-www-form-urlencoded`. Choose one body encoding. |
| `base_url` | Optional operation-specific server supported by published evidence. |
| `transport` | Omit or use `http` for HTTP. `browser` is for rendered HTML GET reads. |
| `response` | Expected format, required structure and optional HTML extraction. |

Input definitions contain `type` (`string`, `int`, `float`, `bool`, `json`), `required`, and optional `default`. Required inputs have no defaults. `json` accepts a nested object or array as one input, such as a GraphQL `variables` object. Input names are exact: a literal hyphenated name is preserved. CLI `--arg 'name=value'` splits on the first equals sign.

GraphQL query documents are parsed independently of the endpoint's name. One evidenced query can run as a read; mutations, subscriptions, malformed candidate documents and ambiguous multi-operation documents require review. Form search endpoints can also use a documented POST read.

Use documented page, cursor, offset and limit inputs to navigate results. Return page information or an observed next-page link and continue only as far as the user's task requires. Do not invent cursor values or unobserved endpoints.

## HTML results

An HTML action declares `response.format: html` and `required_html_fields`: observed `{selector, attribute}` fields that must each select a unique, nonempty value. Empty `attribute` means text. Choose a page-specific field that distinguishes useful content from a login page.

`response.html` can turn a page into structured output:

```yaml
response:
  format: html
  required_html_fields:
    - selector: main h1
  html:
    items: article.reference
    fields:
      title:
        selector: h2 a
        required: true
      url:
        selector: h2 a
        attribute: href
        absolute_url: true
    page:
      next_url:
        selector: a.next
        attribute: href
        absolute_url: true
    limit: 100
```

These selectors are illustrative, not installed website definitions. Selectors must come from the observed page. `items` selects repeated records; fields are relative to each record. An empty field selector selects the record itself. `page` extracts page-level values. Missing optional fields become null. Missing or ambiguous required fields fail validation of the response. `absolute_url` resolves safe HTTP(S) links against the final response URL. Results are bounded to 100 records by default and at most 1,000 when explicitly configured.

For JavaScript-rendered pages, set `transport: browser`, `method: GET` and the same HTML expectations. Hycli discovers a local Chromium engine and restores the selected verified session if needed. It navigates and extracts rendered content; it does not click buttons or submit browser forms. Raw HTTP remains available without a browser engine.

## Accounts and secrets

Public operations omit `auth`. Header authentication declares a named strategy with `kind: header`, the documented `header`, optional `prefix`, and `value_from: store:SITE/KEY`. The user supplies the value in **Website API keys**. A browser strategy uses `kind: browser` and a verified account from the local vault. Operations refer to a strategy by name.

An operation on a different API origin does not inherit the website's cookies or token. Header authentication has its own exact origin scope. A website's browser login cannot substitute for a separately documented API key. Generated recipes reference local key names and structure while credential values stay outside model prompts and exports.

## Verification and limits

JSON expectations use `format: json` and observed JSON pointers in `required_pointers`. Other formats are `text`, `xml`, and `empty`. A non-success HTTP response or a successful response of the wrong expected shape fails the action and CLI exit status. Preparation never verifies a mutation by running it.

The current request contract covers JSON and URL-encoded bodies, typed path/query inputs, HTML extraction and rendered GET reads. Multipart uploads, binary file downloads, arbitrary browser click sequences and automatic streaming are not implemented. Preparation must report a concrete missing capability for those workflows; it must not turn them into a fictitious JSON request or claim completion.

use std::collections::BTreeMap;

use crate::apperr;
use crate::spec;

const GOOD: &str = r##"
spec_version: 1
site:
  name: t1
  title: Test One
  base_url: https://api.example.com
auth:
  - {name: public, kind: none}
  - {name: bearer, kind: header, header: Authorization, prefix: "Bearer ", value_from: "store:t1/token"}
operations:
  - name: item
    method: GET
    path: /items/{id}
    auth: public
    params: {id: {type: int, required: true}}
    query: {verbose: {type: bool}, limit: {type: int, default: 10}}
  - name: create
    method: POST
    path: /items
    auth: bearer
    body:
      json: {name: {type: string, required: true}, count: {type: int, default: 1}}
models:
  item_view:
    id: {type: int}
    name: {type: string}
"##;

fn parse(s: &str) -> spec::Spec {
    spec::parse(s.as_bytes()).unwrap()
}

#[test]
fn parse_validate_good() {
    let sp = parse(GOOD);
    assert!(sp.validate().is_empty(), "issues: {:?}", sp.validate());
}

#[test]
fn validate_catches() {
    let bad = r##"
spec_version: 1
site: {name: t2, title: T, base_url: https://x.example.com}
auth:
  - {name: public, kind: none}
operations:
  - name: a
    method: GET
    path: /x/{id}
    auth: ghost
  - name: b
    method: GET
    path: /y
    auth: public
    query: {limit: {type: int}}
    body:
      json: {limit: {type: int}}
"##;
    let sp = parse(bad);
    let issues = sp.validate();
    let joined = issues
        .iter()
        .map(|i| format!("{} {}", i.field, i.detail))
        .collect::<String>();
    assert!(joined.contains("params"), "missing params issue: {joined}");
    assert!(joined.contains("ghost"), "missing auth issue: {joined}");
    assert!(
        joined.contains("conflict"),
        "missing conflict issue: {joined}"
    );
}

#[test]
fn build_request() {
    let sp = parse(GOOD);
    let op = sp.op("item").unwrap();
    let mut args = BTreeMap::new();
    args.insert("id".to_string(), "42".to_string());
    let (url, body) = sp.build_request(op, &args).unwrap();
    assert_eq!(url, "https://api.example.com/items/42?limit=10");
    assert!(body.is_none());

    let err = sp.build_request(op, &Default::default()).unwrap_err();
    assert!(err.msg.contains("id"), "missing required message: {err}");

    let op2 = sp.op("create").unwrap();
    let mut args = BTreeMap::new();
    args.insert("name".to_string(), "logo".to_string());
    args.insert("count".to_string(), "3".to_string());
    let (_, body2) = sp.build_request(op2, &args).unwrap();
    let m: serde_json::Value = serde_json::from_slice(&body2.unwrap()).unwrap();
    assert_eq!(m["name"], "logo");
    assert!(m["count"].is_number());
}

#[test]
fn build_multipart_resolves_file_and_text_parts() {
    let sp = parse(
        r##"
spec_version: 1
site:
  name: up
  title: Upload Fixture
  base_url: https://up.example.com
operations:
  - name: upload
    method: POST
    path: /upload
    body:
      multipart:
        avatar: {type: file, required: true}
        caption: {type: string, default: hello}
        slot: {type: string, required: true}
"##,
    );
    let op = sp.op("upload").unwrap();
    let mut args = BTreeMap::new();
    args.insert("avatar".to_string(), "/tmp/pic.gif".to_string());
    args.insert("slot".to_string(), "main".to_string());
    let parts = sp.build_multipart(op, &args).unwrap();
    // BTreeMap iterates keys sorted: avatar, caption, slot.
    assert_eq!(parts.len(), 3);
    match &parts[0] {
        spec::MultipartPart::File { name, path } => {
            assert_eq!(name, "avatar");
            assert_eq!(path, "/tmp/pic.gif");
        }
        other => panic!("expected file part, got {other:?}"),
    }
    match &parts[1] {
        spec::MultipartPart::Text { name, value } => {
            assert_eq!(name, "caption");
            assert_eq!(value, "hello", "default applied");
        }
        other => panic!("expected text part, got {other:?}"),
    }
    match &parts[2] {
        spec::MultipartPart::Text { name, value } => {
            assert_eq!(name, "slot");
            assert_eq!(value, "main");
        }
        other => panic!("expected text part, got {other:?}"),
    }
    let err = sp.build_multipart(op, &BTreeMap::new()).unwrap_err();
    assert!(err.msg.contains("avatar"), "missing required: {err}");
}

#[test]
fn apply_model() {
    let sp = parse(GOOD);
    let raw = br#"{"id": 7, "name": "abc", "extra": true}"#;
    let out = sp.apply_model("item_view", raw).unwrap();
    assert_eq!(out["id"], 7);
    assert_eq!(out["name"], "abc");
    assert!(
        out.get("extra").is_none(),
        "unexpected extra field in model output"
    );
}

#[test]
fn op_not_found_lists_names() {
    let sp = parse(GOOD);
    let err = sp.op("nope").unwrap_err();
    assert!(err.remedy.contains("item"));
    let _ = apperr::Code::Ok;
}

#[test]
fn v2_json_template_and_response_preprocessing() {
    let raw_spec = r##"
spec_version: 2
site:
  name: v2site
  title: V2 Site
  base_url: https://app.example.com
auth:
  - name: xsrf
    kind: derived
    header: X-CSRF-Token
    value_from: cookie:ct0
operations:
  - name: next-feed
    method: GET
    path: /feed
    auth: xsrf
    response:
      format: json
      ssr_json: __NEXT_DATA__
      extract_pointer: /props/pageProps/items
      required_pointers:
        - /0/id
  - name: create-post
    method: POST
    path: /api/posts
    auth: xsrf
    preflight:
      - name: csrf
        path: /feed
        extract:
          token: "css:meta[name='csrf-token']@content"
    body:
      json:
        title: {type: string, required: true}
        limit: {type: int, default: 5}
      json_template:
        operationName: CreatePost
        variables:
          input:
            title: "{{title}}"
            limit: "{{limit}}"
            csrf: "{{step.csrf.token}}"
    response:
      format: json
      strip_prefix: ")]}'\n"
      extract_pointer: /data/post
"##;
    let sp = parse(raw_spec);
    assert!(
        sp.validate().is_empty(),
        "v2 validate issues: {:?}",
        sp.validate()
    );

    let create_op = sp.op("create-post").unwrap();
    let mut args = BTreeMap::new();
    args.insert("title".into(), "Hello".into());
    args.insert("step.csrf.token".into(), "tok-123".into());
    let (_, body_bytes) = sp.build_request(create_op, &args).unwrap();
    let body_json: serde_json::Value = serde_json::from_slice(&body_bytes.unwrap()).unwrap();
    assert_eq!(body_json["operationName"], "CreatePost");
    assert_eq!(body_json["variables"]["input"]["title"], "Hello");
    assert_eq!(body_json["variables"]["input"]["limit"], 5);
    assert_eq!(body_json["variables"]["input"]["csrf"], "tok-123");

    let xssi_resp = crate::net::Response {
        url: "https://app.example.com/api/posts".into(),
        status: 200,
        headers: vec![("content-type".into(), "application/json".into())],
        bytes: b")]}'\n{\"data\":{\"post\":{\"id\":\"p-1\"}}}".to_vec(),
        duration_ms: 10,
        defense: "none".into(),
    };
    assert!(create_op.matches_response(&xssi_resp));
    assert_eq!(
        create_op.parse_json_response(&xssi_resp).unwrap()["id"],
        "p-1"
    );

    let next_op = sp.op("next-feed").unwrap();
    let ssr_resp = crate::net::Response {
        url: "https://app.example.com/feed".into(),
        status: 200,
        headers: vec![("content-type".into(), "text/html".into())],
        bytes: br#"<html><body><script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"items":[{"id":"clip-7"}]}}}</script></body></html>"#.to_vec(),
        duration_ms: 12,
        defense: "none".into(),
    };
    assert!(next_op.matches_response(&ssr_resp));
    assert_eq!(
        next_op.parse_json_response(&ssr_resp).unwrap()[0]["id"],
        "clip-7"
    );
}

#[test]
fn multipart_file_defaults_are_rejected_even_when_optional() {
    for required in [false, true] {
        let sp = parse(&serde_json::json!({
            "spec_version":1,"site":{"name":"fixture","base_url":"https://example.com"},
            "operations":[{"name":"upload","method":"POST","path":"/put","effect":"write",
                "body":{"multipart":{"file":{"type":"file","required":required,"default":"/synthetic/not-opened"}}}}]
        }).to_string());
        assert!(!sp.validate().is_empty());
        assert!(
            sp.build_multipart(sp.op("upload").unwrap(), &BTreeMap::new())
                .is_err()
        );
    }
}

#[test]
fn multipart_file_is_write_even_for_read_search_and_missing_optional_file() {
    let sp = parse(
        &serde_json::json!({
            "spec_version":1,"site":{"name":"fixture","base_url":"https://example.com"},
            "operations":[{"name":"search","method":"POST","path":"/search","effect":"read",
                "body":{"multipart":{"file":{"type":"file"}}}}]
        })
        .to_string(),
    );
    let op = sp.op("search").unwrap();
    assert_eq!(
        crate::policy::operation_effect(op, &BTreeMap::new()),
        crate::policy::Effect::Write
    );
    assert!(sp.build_multipart(op, &BTreeMap::new()).unwrap().is_empty());
}

#[test]
fn final_derived_target_is_required_and_preserves_the_exact_endpoint() {
    let sp = parse(
        &serde_json::json!({
            "spec_version":3,"site":{"name":"fixture","base_url":"https://example.com"},
            "operations":[{"name":"upload","method":"POST","path":"","derived_target":true,
                "base_url":"{{step.lease.target}}","effect":"write"}]
        })
        .to_string(),
    );
    assert!(sp.validate().is_empty());
    let op = sp.op("upload").unwrap();
    assert!(sp.build_final_request(op, &BTreeMap::new()).is_err());
    for target in [
        "https://storage.example.test/object-id",
        "https://storage.example.test/object-id?signature=abc%2Fdef",
    ] {
        let args = BTreeMap::from([("step.lease.target".into(), target.into())]);
        assert_eq!(sp.build_final_request(op, &args).unwrap().0, target);
    }
    assert!(
        sp.build_final_request(
            op,
            &BTreeMap::from([("step.lease.target".into(), "not-a-url".into())])
        )
        .is_err()
    );
}

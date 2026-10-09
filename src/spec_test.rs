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
    assert!(sp.validate().is_empty(), "이슈: {:?}", sp.validate());
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
    assert!(joined.contains("params"), "params 이슈 누락: {joined}");
    assert!(joined.contains("ghost"), "auth 이슈 누락: {joined}");
    assert!(joined.contains("충돌"), "충돌 이슈 누락: {joined}");
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
    assert!(err.msg.contains("id"), "필수 누락 메시지: {err}");

    let op2 = sp.op("create").unwrap();
    let mut args = BTreeMap::new();
    args.insert("name".to_string(), "로고".to_string());
    args.insert("count".to_string(), "3".to_string());
    let (_, body2) = sp.build_request(op2, &args).unwrap();
    let m: serde_json::Value = serde_json::from_slice(&body2.unwrap()).unwrap();
    assert_eq!(m["name"], "로고");
    assert!(m["count"].is_number());
}

#[test]
fn apply_model() {
    let sp = parse(GOOD);
    let raw = br#"{"id": 7, "name": "abc", "extra": true}"#;
    let out = sp.apply_model("item_view", raw).unwrap();
    assert_eq!(out["id"], 7);
    assert_eq!(out["name"], "abc");
    assert!(out.get("extra").is_none(), "모델에 없는 필드가 샜다");
}

#[test]
fn op_not_found_lists_names() {
    let sp = parse(GOOD);
    let err = sp.op("nope").unwrap_err();
    assert!(err.remedy.contains("item"));
    let _ = apperr::Code::Ok;
}

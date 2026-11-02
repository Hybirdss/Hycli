use axum::{Json, Router, extract::Query, routing::get};
use hycli::{policy, runtime::Runtime, spec, util};
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};
fn definition(origin: &str) -> spec::Spec {
    spec::parse(json!({"spec_version":3,"site":{"name":"pagination-repro","base_url":origin},"operations":[{"name":"list-items","method":"GET","path":"/items","effect":"read","evidence":"Synthetic pagination documentation","query":{"q":{"type":"string"}},"pagination":{"input":"cursor","query":"pageToken","next":"/nextPageToken"}}]}).to_string().as_bytes()).unwrap()
}
fn runtime() -> Arc<Runtime> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/pagination-tests")
        .join(util::id());
    Runtime::open(&root, None, None, false, true, false).unwrap()
}
async fn fixture() -> (String, tokio::task::JoinHandle<()>) {
    let app=Router::new().route("/items",get(|Query(q):Query<BTreeMap<String,String>>|async move{Json(if q.get("pageToken").map(String::as_str)==Some("synthetic-opaque-page-two"){json!({"items":[2],"echo":"synthetic-opaque-page-two"})}else{json!({"items":[1],"nextPageToken":"synthetic-opaque-page-two","echo":"synthetic-opaque-page-two"})})}));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    (
        url,
        tokio::spawn(async { axum::serve(l, app).await.unwrap() }),
    )
}
#[test]
fn metadata_is_explicit_and_does_not_relax_global_secrets() {
    let sp = definition("https://example.com");
    assert!(policy::validate_spec(&sp).is_ok());
    assert!(util::secret_field("pageToken"));
    assert!(util::secret_field("accessToken"));
    for bad in ["accessToken", "authorization", "delete"] {
        let mut v = sp.clone();
        v.operations[0].pagination.as_mut().unwrap().query = bad.into();
        assert!(policy::validate_spec(&v).is_err());
    }
    for version in [1, 2] {
        let mut v = sp.clone();
        v.spec_version = version;
        assert!(policy::validate_spec(&v).is_err());
        v.operations[0].pagination = None;
        assert!(policy::validate_spec(&v).is_ok());
    }
    for key in ["cursor", "pageToken"] {
        let mut v = sp.clone();
        v.operations[0].query.insert(
            key.into(),
            serde_json::from_value(json!({"type":"string"})).unwrap(),
        );
        assert!(policy::validate_spec(&v).is_err());
    }
    let mut v = sp.clone();
    v.operations[0].path = "/items/{cursor}".into();
    assert!(policy::validate_spec(&v).is_err());
    let mut v = sp.clone();
    v.operations[0]
        .headers
        .insert("x-page".into(), "{{cursor}}".into());
    assert!(policy::validate_spec(&v).is_err());
    let mut v = sp.clone();
    v.operations[0].transport = "browser-fetch".into();
    assert!(policy::validate_spec(&v).is_err());
    let mut v = sp.clone();
    v.operations[0].pagination.as_mut().unwrap().next = "/accessToken".into();
    assert!(policy::validate_spec(&v).is_err());
}
#[tokio::test]
async fn opaque_handles_survive_restart_but_not_request_or_credential_changes() {
    let (url, server) = fixture().await;
    let core = runtime();
    let sp = definition(&url);
    core.install(&sp).unwrap();
    let first = core
        .run_op(&sp, "list-items", &BTreeMap::new())
        .await
        .unwrap()
        .0;
    let handle = first.next_cursor.clone().unwrap();
    assert!(
        !serde_json::to_string(&first)
            .unwrap()
            .contains("synthetic-opaque-page-two")
    );
    assert_eq!(first.clone().output().unwrap()["next_cursor"], handle);
    let root = core.root.clone();
    drop(core);
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    let args = BTreeMap::from([("cursor".into(), handle.clone())]);
    let second = core.run_op(&sp, "list-items", &args).await.unwrap().0;
    assert_eq!(second.body.as_ref().unwrap()["items"], json!([2]));
    assert!(second.next_cursor.is_none());
    assert!(
        !serde_json::to_string(&second)
            .unwrap()
            .contains("synthetic-opaque-page-two")
    );
    // Same read can be retried; a changed request cannot spend its capability.
    assert!(core.run_op(&sp, "list-items", &args).await.is_ok());
    let mut changed = args.clone();
    changed.insert("q".into(), "other".into());
    assert!(core.request(&sp, "list-items", changed, None).is_err());
    let fresh = core
        .run_op(&sp, "list-items", &BTreeMap::new())
        .await
        .unwrap()
        .0
        .next_cursor
        .unwrap();
    core.vault
        .lock()
        .unwrap()
        .set("pagination-repro", "api-key", "synthetic-auth-secret")
        .unwrap();
    assert!(
        core.request(
            &sp,
            "list-items",
            BTreeMap::from([("cursor".into(), fresh)]),
            None
        )
        .is_err()
    );
    assert!(
        core.request(
            &sp,
            "list-items",
            BTreeMap::from([("cursor".into(), "raw-secret-not-handle".into())]),
            None
        )
        .is_err()
    );
    server.abort();
}

#[tokio::test]
async fn mapped_outputs_and_redirects_cannot_export_private_continuations() {
    use axum::http::StatusCode;
    let app=Router::new().route("/items",get(|Query(q):Query<BTreeMap<String,String>>|async move{
  if q.contains_key("pageToken") {(StatusCode::FOUND,[("location","/should-not-follow")],Json(json!({"echo":"synthetic-next-cursor"})))}else{(StatusCode::OK,[("location","")],Json(json!({"items":[1],"nextPageToken":"synthetic-next-cursor","echo":"synthetic-next-cursor"})))}
 })).route("/should-not-follow",get(||async{panic!("pagination followed a redirect"); #[allow(unreachable_code)] Json(json!({}))}));
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let server = tokio::spawn(async { axum::serve(l, app).await.unwrap() });
    let core = runtime();
    let mut value = serde_json::to_value(definition(&origin)).unwrap();
    value["operations"][0]["model"] = json!("page");
    value["models"] = json!({"page":{"copy":{"path":"echo","type":"string"}}});
    let sp = spec::parse(value.to_string().as_bytes()).unwrap();
    core.install(&sp).unwrap();
    let first = core
        .run_op(&sp, "list-items", &BTreeMap::new())
        .await
        .unwrap()
        .0;
    assert!(
        !serde_json::to_string(&first)
            .unwrap()
            .contains("synthetic-next-cursor")
    );
    let error = core
        .run_op(
            &sp,
            "list-items",
            &BTreeMap::from([("cursor".into(), first.next_cursor.unwrap())]),
        )
        .await
        .err()
        .expect("redirect must not become a successful action");
    assert_eq!(error.public_code, "request_failed");
    assert!(!format!("{error:?}").contains("synthetic-next-cursor"));
    assert_public_files_clean(&core.root, "synthetic-next-cursor");
    server.abort();
}

fn assert_public_files_clean(root: &std::path::Path, private: &str) {
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() && path.file_name().unwrap() != "store.json" {
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                !bytes
                    .windows(private.len())
                    .any(|w| w == private.as_bytes()),
                "public file {:?} leaked cursor",
                path.file_name()
            );
        }
    }
}

#[tokio::test]
async fn rejected_responses_and_transport_resets_never_persist_cursors() {
    use axum::http::StatusCode;
    for bad in [
        json!({"private":"rejected-cursor-marker"}),
        json!("rejected-cursor-marker".repeat(300)),
    ] {
        let app = Router::new().route(
            "/items",
            get(move || {
                let bad = bad.clone();
                async move { Json(json!({"nextPageToken":bad})) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let sp = definition(&format!("http://{}", listener.local_addr().unwrap()));
        let server = tokio::spawn(async { axum::serve(listener, app).await.unwrap() });
        let core = runtime();
        core.install(&sp).unwrap();
        let error = core
            .run_op(&sp, "list-items", &BTreeMap::new())
            .await
            .err()
            .unwrap();
        assert_eq!(error.public_code, "pagination_invalid");
        assert!(!format!("{error:?}").contains("rejected-cursor-marker"));
        assert_public_files_clean(&core.root, "rejected-cursor-marker");
        server.abort();
    }
    let app=Router::new().route("/items",get(|Query(q):Query<BTreeMap<String,String>>|async move {
        if q.contains_key("pageToken") {(StatusCode::BAD_REQUEST,Json(json!({"echo":"private-error-cursor","nextPageToken":"outgoing-error-cursor"})))}
        else {(StatusCode::OK,Json(json!({"items":[1],"nextPageToken":"private-error-cursor"})))}
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let sp = definition(&format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async { axum::serve(listener, app).await.unwrap() });
    let core = runtime();
    core.install(&sp).unwrap();
    let handle = core
        .run_op(&sp, "list-items", &BTreeMap::new())
        .await
        .unwrap()
        .0
        .next_cursor
        .unwrap();
    let args = BTreeMap::from([("cursor".into(), handle)]);
    let error = core.run_op(&sp, "list-items", &args).await.err().unwrap();
    assert_eq!(error.public_code, "request_failed");
    for value in ["private-error-cursor", "outgoing-error-cursor"] {
        assert!(!format!("{error:?}").contains(value));
        assert_public_files_clean(&core.root, value);
    }
    server.abort();
    // A closed endpoint exercises reqwest connect/reset errors with the private query present.
    let error = core.run_op(&sp, "list-items", &args).await.err().unwrap();
    assert_eq!(error.public_code, "request_failed");
    assert!(!format!("{error:?}").contains("private-error-cursor"));
    assert_public_files_clean(&core.root, "private-error-cursor");
}

#[tokio::test]
async fn continuation_scrubbing_does_not_rewrite_legacy_results_or_models() {
    let app=Router::new()
        .route("/items",get(|Query(q):Query<BTreeMap<String,String>>|async move {
            Json(json!({"nextPageToken":if q.contains_key("q") {"report-2026"}else{"2"},"year":"2026","v2":"untouched"}))
        }))
        .route("/legacy",get(||async {Json(json!({"year":"2026","report-2026":"keep-report-2026"}))}));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async { axum::serve(listener, app).await.unwrap() });
    let core = runtime();
    let sp = definition(&origin);
    core.install(&sp).unwrap();
    let error = core
        .run_op(&sp, "list-items", &BTreeMap::new())
        .await
        .err()
        .unwrap();
    assert_eq!(error.public_code, "pagination_invalid");
    let valid = core
        .run_op(
            &sp,
            "list-items",
            &BTreeMap::from([("q".into(), "valid".into())]),
        )
        .await
        .unwrap()
        .0;
    assert!(valid.next_cursor.is_some());
    assert_eq!(valid.body.as_ref().unwrap()["year"], "2026");
    assert_eq!(valid.body.as_ref().unwrap()["v2"], "untouched");
    let legacy=spec::parse(json!({"spec_version":1,"site":{"name":"legacy-repro","base_url":origin},"models":{"record":{"year":{"path":"year","type":"string"},"version":{"path":"report-2026","type":"string"}}},"operations":[{"name":"list-items","method":"GET","path":"/legacy","effect":"read","evidence":"Synthetic read documentation","model":"record"}]}).to_string().as_bytes()).unwrap();
    core.install(&legacy).unwrap();
    let result = core
        .run_op(&legacy, "list-items", &BTreeMap::new())
        .await
        .unwrap()
        .0;
    assert_eq!(
        result.body.unwrap(),
        json!({"year":"2026","report-2026":"keep-report-2026"})
    );
    assert_eq!(
        result.mapped.unwrap(),
        json!({"year":"2026","version":"keep-report-2026"})
    );
    server.abort();
}

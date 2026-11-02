use super::*;
use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[derive(Default)]
struct Fixture {
    exchanges: AtomicUsize,
    reads: AtomicUsize,
    identities: AtomicUsize,
    mode: AtomicUsize,
}

async fn fixture() -> (String, Arc<Fixture>, tokio::task::JoinHandle<()>) {
    fixture_on(tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap()).await
}

async fn fixture_on(
    listener: tokio::net::TcpListener,
) -> (String, Arc<Fixture>, tokio::task::JoinHandle<()>) {
    let state = Arc::new(Fixture::default());
    let token_state = state.clone();
    let identity_state = state.clone();
    let resource_state = state.clone();
    let router = Router::new()
        .route("/token", post(move |body: String| { let s = token_state.clone(); async move {
            let count = s.exchanges.fetch_add(1, Ordering::SeqCst);
            let fields = url::form_urlencoded::parse(body.as_bytes()).into_owned().collect::<Vec<_>>();
            assert_eq!(fields, vec![
                ("grant_type".into(), "refresh_token".into()),
                ("refresh_token".into(), if count == 0 { "refresh-canary-old" } else { "refresh-canary-new" }.into()),
                ("client_id".into(), "client-canary".into()),
                ("client_secret".into(), "secret-canary".into()),
            ]);
            match s.mode.load(Ordering::SeqCst) {
                3 => return (StatusCode::TEMPORARY_REDIRECT, [("location", "/token-destination")], "").into_response(),
                4 => return (StatusCode::OK, "not json").into_response(),
                5 => return (StatusCode::OK, "x".repeat(70000)).into_response(),
                14 => return axum::response::Response::new(axum::body::Body::from_stream(futures_util::stream::once(async { Err::<Vec<u8>, _>(std::io::Error::other("synthetic lost token response")) }))),
                9 => tokio::time::sleep(Duration::from_millis(250)).await,
                15 => tokio::time::sleep(Duration::from_secs(60)).await,
                _ => {}
            }
            let mut value = json!({"access_token":"access-canary-new", "refresh_token":"refresh-canary-new", "expires_in":3600, "token_type":"Bearer", "scope":"read"});
            if s.mode.load(Ordering::SeqCst) == 1 { value.as_object_mut().unwrap().remove("refresh_token"); value.as_object_mut().unwrap().remove("scope"); }
            if s.mode.load(Ordering::SeqCst) == 2 { value["scope"] = json!("read write"); }
            Json(value).into_response()
        }}))
        .route("/token-destination", post(|| async { panic!("token redirect was followed"); #[allow(unreachable_code)] "" }))
        .route("/identity", get(move |headers: HeaderMap| { let s = identity_state.clone(); async move {
            assert_eq!(headers["authorization"], if s.exchanges.load(Ordering::SeqCst) > 0 { "Bearer access-canary-new" } else { "Bearer access-canary-old" });
            s.identities.fetch_add(1, Ordering::SeqCst);
            if s.mode.load(Ordering::SeqCst) == 7 { return StatusCode::SERVICE_UNAVAILABLE.into_response(); }
            Json(json!({"sub":if s.mode.load(Ordering::SeqCst) == 6 { "changed-subject" } else { "subject-canary" }})).into_response()
        }}))
        .route("/items", get(move |headers: HeaderMap, axum::extract::Query(query): axum::extract::Query<BTreeMap<String,String>>| { let s = resource_state.clone(); async move {
            s.reads.fetch_add(1, Ordering::SeqCst);
            let mode = s.mode.load(Ordering::SeqCst);
            if mode == 8 { return (StatusCode::FORBIDDEN, Json(json!({"error":"insufficient_scope"}))).into_response(); }
            if mode == 12 || mode == 13 { tokio::time::sleep(Duration::from_millis(350)).await; }
            if mode == 10 || ((mode == 11 || mode == 12 || mode == 16) && headers.get("authorization").unwrap() == "Bearer access-canary-old") {
                return (StatusCode::UNAUTHORIZED, [("www-authenticate", "Bearer error=\"invalid_token\"")], Json(json!({"error":"expired"}))).into_response();
            }
            if mode == 16 {
                return axum::http::Response::builder().header("content-type", "application/pdf")
                    .body(axum::body::Body::from_stream(futures_util::stream::iter((0..1601).map(|_| Ok::<_, std::io::Error>(vec![0x61; crate::artifacts::TRANSFER_CHUNK_BYTES]))))).unwrap();
            }
            let page_two = query.get("pageToken").is_some();
            if page_two { assert_eq!(query.get("pageToken").unwrap(), "opaque-page-two-canary"); }
            Json(json!({"subject":"subject-canary","client_id":"client-canary","items":[if page_two { 2 } else { 1 }],"nextPage":if page_two { Value::Null } else { json!("opaque-page-two-canary") },"echo":format!("{} {} client-canary secret-canary subject-canary", headers["authorization"].to_str().unwrap(), if s.exchanges.load(Ordering::SeqCst) > 0 && mode != 1 { "refresh-canary-new" } else { "refresh-canary-old" })})).into_response()
        }}));
    let origin = format!("http://{}", listener.local_addr().unwrap());
    (
        origin,
        state,
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        }),
    )
}
fn definition(origin: &str) -> Spec {
    crate::spec::parse(json!({"spec_version":3,"site":{"name":"oauth-fixture","base_url":origin},"auth":[{"name":"api","kind":"header","header":"authorization","prefix":"Bearer ","origin":origin,"oauth_refresh":{"grant_ref":"grant-one","token_endpoint":format!("{origin}/token"),"identity_endpoint":format!("{origin}/identity"),"subject_pointer":"/sub","resource":"synthetic-resource","scopes":["read"]}}],"operations":[{"name":"read-items","method":"GET","path":"/items","auth":"api","effect":"read","evidence":"Synthetic OAuth fixture documentation"}]}).to_string().as_bytes()).unwrap()
}
fn grant_input(expired: bool) -> String {
    json!({"kind":"oauth_refresh","client_id":"client-canary","client_secret":"secret-canary","access_token":"access-canary-old","refresh_token":"refresh-canary-old","expires_at": now() + if expired { -100 } else { 3600 },"subject":"subject-canary"}).to_string()
}
fn root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/oauth-tests")
        .join(util::id())
}
fn configured(origin: &str, expired: bool) -> (Arc<Runtime>, Spec) {
    let core = Runtime::open_oauth_test(&root()).unwrap();
    let sp = definition(origin);
    core.install(&sp).unwrap();
    core.set_website_credential(&sp.site.name, "api", Some(&grant_input(expired)))
        .unwrap();
    (core, sp)
}
async fn read(core: &Runtime, sp: &Spec) -> AppResult<super::super::OpResult> {
    core.run_op(sp, "read-items", &BTreeMap::new())
        .await
        .map(|(r, _)| r)
}
fn stored(core: &Runtime, sp: &Spec) -> Grant {
    core.oauth_load(
        &sp.auth[0]
            .oauth_refresh
            .as_ref()
            .unwrap()
            .key(&sp.site.name),
    )
    .unwrap()
    .0
}
fn expire(core: &Runtime, sp: &Spec) {
    let key = sp.auth[0]
        .oauth_refresh
        .as_ref()
        .unwrap()
        .key(&sp.site.name);
    let _lock = core.oauth_lock(&key).unwrap();
    let (mut grant, raw) = core.oauth_load(&key).unwrap();
    grant.input.expires_at = now() - 100;
    core.oauth_commit(&key, Some(&raw), Some(&grant)).unwrap();
}
#[tokio::test]
async fn valid_grant_has_zero_exchanges_and_redacts_individual_fields() {
    let (url, state, server) = fixture().await;
    let (core, mut sp) = configured(&url, false);
    sp.models.insert("identity".into(), serde_json::from_value(json!({"id":{"type":"string","path":"subject"},"client":{"type":"string","path":"client_id"}})).unwrap());
    sp.operations[0].model = "identity".into();
    core.install(&sp).unwrap();
    let result = read(&core, &sp).await.unwrap();
    let output = serde_json::to_string(&result).unwrap();
    for secret in ["access-canary-old", "refresh-canary-old", "secret-canary"] {
        assert!(!output.contains(secret), "{secret}");
    }
    assert_eq!(result.body.as_ref().unwrap()["subject"], "subject-canary");
    assert_eq!(result.body.as_ref().unwrap()["client_id"], "client-canary");
    assert_eq!(result.mapped.as_ref().unwrap()["id"], "subject-canary");
    assert_eq!(result.mapped.as_ref().unwrap()["client"], "client-canary");
    assert_eq!(state.exchanges.load(Ordering::SeqCst), 0);
    assert_eq!(state.reads.load(Ordering::SeqCst), 1);
    assert!(
        !std::fs::read_to_string(core.root.join("store.json"))
            .unwrap()
            .contains("canary")
    );
    server.abort();
}
#[tokio::test]
async fn expired_rotation_is_atomic_and_survives_restart() {
    let (url, state, server) = fixture().await;
    let (core, sp) = configured(&url, true);
    read(&core, &sp).await.unwrap();
    let grant = stored(&core, &sp);
    assert_eq!(grant.input.refresh_token, "refresh-canary-new");
    assert_eq!(grant.input.access_token, "access-canary-new");
    assert_eq!(grant.generation, 1);
    let reopened = Runtime::open_oauth_test(&core.root).unwrap();
    read(&reopened, &sp).await.unwrap();
    assert_eq!(state.exchanges.load(Ordering::SeqCst), 1);
    server.abort();
}
#[tokio::test]
async fn omitted_scope_and_refresh_token_retain_the_approved_grant() {
    let (url, state, server) = fixture().await;
    state.mode.store(1, Ordering::SeqCst);
    let (core, sp) = configured(&url, true);
    read(&core, &sp).await.unwrap();
    assert_eq!(stored(&core, &sp).input.refresh_token, "refresh-canary-old");
    assert_eq!(state.exchanges.load(Ordering::SeqCst), 1);
    server.abort();
}
#[tokio::test]
async fn scope_identity_redirect_malformed_and_oversize_fail_before_resource_use() {
    for mode in [2, 3, 4, 5, 6, 14] {
        let (url, state, server) = fixture().await;
        state.mode.store(mode, Ordering::SeqCst);
        let (core, sp) = configured(&url, true);
        assert!(read(&core, &sp).await.is_err(), "mode {mode}");
        let reopened = Runtime::open_oauth_test(&core.root).unwrap();
        assert!(read(&reopened, &sp).await.is_err(), "mode {mode}");
        assert_eq!(state.exchanges.load(Ordering::SeqCst), 1, "mode {mode}");
        assert_eq!(state.reads.load(Ordering::SeqCst), 0, "mode {mode}");
        server.abort();
    }
}
#[tokio::test]
async fn pending_identity_retries_verification_not_rotation() {
    let (url, state, server) = fixture().await;
    state.mode.store(7, Ordering::SeqCst);
    let (core, sp) = configured(&url, true);
    assert!(read(&core, &sp).await.is_err());
    assert!(stored(&core, &sp).phase == Phase::VerificationPending);
    state.mode.store(0, Ordering::SeqCst);
    read(&core, &sp).await.unwrap();
    assert_eq!(state.exchanges.load(Ordering::SeqCst), 1);
    server.abort();
}
#[tokio::test]
async fn storage_failures_never_replay_a_consumed_token() {
    for (remaining, after) in [(0, false), (1, false), (1, true)] {
        let (url, state, server) = fixture().await;
        let (core, sp) = configured(&url, true);
        core.vault.lock().unwrap().oauth_fail_after = Some((remaining, after));
        assert!(read(&core, &sp).await.is_err());
        assert_eq!(
            state.exchanges.load(Ordering::SeqCst),
            usize::from(remaining != 0)
        );
        if remaining == 1 {
            let reopened = Runtime::open_oauth_test(&core.root).unwrap();
            let result = read(&reopened, &sp).await;
            assert_eq!(result.is_ok(), after);
            assert_eq!(state.exchanges.load(Ordering::SeqCst), 1);
        }
        server.abort();
    }
}
#[tokio::test]
async fn forbidden_never_refreshes_and_expired_bearer_has_one_budget() {
    for (mode, expired, exchanges, reads) in [
        (8, false, 0, 1),
        (11, false, 1, 2),
        (10, true, 1, 1),
        (10, false, 1, 2),
    ] {
        let (url, state, server) = fixture().await;
        state.mode.store(mode, Ordering::SeqCst);
        let (core, sp) = configured(&url, expired);
        let result = read(&core, &sp).await;
        if mode == 10 {
            assert_eq!(
                result.err().expect("rejected rotated token").public_code,
                "oauth_recovery_required"
            );
            assert_eq!(
                read(&core, &sp)
                    .await
                    .err()
                    .expect("recovery required")
                    .public_code,
                "oauth_recovery_required"
            );
        }
        assert_eq!(state.exchanges.load(Ordering::SeqCst), exchanges);
        assert_eq!(state.reads.load(Ordering::SeqCst), reads);
        server.abort();
    }
}
#[test]
fn existing_private_store_contract_and_typed_input_are_preserved() {
    let core = Runtime::open(&root(), None, None, false, true, false).unwrap();
    let sp = definition("http://127.0.0.1:12345");
    core.install(&sp).unwrap();
    core.set_website_credential(&sp.site.name, "api", Some(&grant_input(false)))
        .unwrap();
    assert_eq!(
        core.vault.lock().unwrap().protection(),
        "file_permissions_only"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(core.root.join("store.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    assert_eq!(
        core.set_website_credential(&sp.site.name, "api", Some("raw-token-canary"))
            .unwrap_err()
            .public_code,
        "oauth_configuration_invalid"
    );
    assert!(
        core.vault
            .lock()
            .unwrap()
            .set("@oauth", "unsafe", "raw-token-canary")
            .is_err()
    );
    let encrypted = Runtime::open_oauth_test(&root()).unwrap();
    encrypted.install(&sp).unwrap();
    encrypted
        .set_website_credential(&sp.site.name, "api", Some(&grant_input(false)))
        .unwrap();
    let before = std::fs::read(encrypted.root.join("store.json")).unwrap();
    assert!(Runtime::open(&encrypted.root, None, None, false, true, false).is_err());
    assert_eq!(
        std::fs::read(encrypted.root.join("store.json")).unwrap(),
        before
    );
}
#[tokio::test]
async fn oauth_child_process() {
    let Some(root) = std::env::var_os("HYCLI_OAUTH_TEST_CHILD_ROOT") else {
        return;
    };
    let core = Runtime::open_oauth_test(std::path::Path::new(&root)).unwrap();
    let sp = core.spec("oauth-fixture").unwrap();
    read(&core, &sp).await.unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn separate_processes_share_one_exchange_and_adopt_rotated_generation() {
    let (url, state, server) = fixture().await;
    state.mode.store(9, Ordering::SeqCst);
    let (core, _) = configured(&url, true);
    let mut first = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "runtime::oauth::tests::oauth_child_process",
            "--nocapture",
        ])
        .env("HYCLI_OAUTH_TEST_CHILD_ROOT", &core.root)
        .spawn()
        .unwrap();
    let mut second = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "runtime::oauth::tests::oauth_child_process",
            "--nocapture",
        ])
        .env("HYCLI_OAUTH_TEST_CHILD_ROOT", &core.root)
        .spawn()
        .unwrap();
    assert!(first.wait().await.unwrap().success());
    assert!(second.wait().await.unwrap().success());
    assert_eq!(state.exchanges.load(Ordering::SeqCst), 1);
    assert_eq!(state.reads.load(Ordering::SeqCst), 2);
    server.abort();
}
#[tokio::test]
async fn verified_rotation_preserves_cursor_but_owner_replacement_does_not() {
    let (url, state, server) = fixture().await;
    let (core, mut sp) = configured(&url, false);
    sp.operations[0].pagination = Some(
        serde_json::from_value(json!({"input":"cursor","query":"pageToken","next":"/nextPage"}))
            .unwrap(),
    );
    core.install(&sp).unwrap();
    let result = read(&core, &sp).await.unwrap();
    let cursor = result.next_cursor.unwrap();
    expire(&core, &sp);
    let (second, _) = core
        .run_op(
            &sp,
            "read-items",
            &BTreeMap::from([("cursor".into(), cursor.clone())]),
        )
        .await
        .unwrap();
    assert_eq!(second.body.unwrap()["items"], json!([2]));
    assert!(second.next_cursor.is_none());
    assert_eq!(state.exchanges.load(Ordering::SeqCst), 1);
    let mut changed = sp.clone();
    changed.auth[0]
        .oauth_refresh
        .as_mut()
        .unwrap()
        .scopes
        .push("write".into());
    core.install(&changed).unwrap();
    assert!(
        core.run_op(
            &changed,
            "read-items",
            &BTreeMap::from([("cursor".into(), cursor.clone())])
        )
        .await
        .is_err()
    );
    assert_eq!(state.reads.load(Ordering::SeqCst), 2);
    core.install(&sp).unwrap();
    core.set_website_credential(&sp.site.name, "api", Some(&grant_input(false)))
        .unwrap();
    assert!(
        core.run_op(
            &sp,
            "read-items",
            &BTreeMap::from([("cursor".into(), cursor)])
        )
        .await
        .is_err()
    );
    assert_eq!(state.reads.load(Ordering::SeqCst), 2);
    server.abort();
}

#[tokio::test]
async fn late_old_generation_401_cannot_poison_a_rotated_grant() {
    let (url, state, server) = fixture().await;
    let (core, sp) = configured(&url, false);
    read(&core, &sp).await.unwrap();
    state.mode.store(12, Ordering::SeqCst);
    let first = tokio::spawn({
        let core = core.clone();
        let sp = sp.clone();
        async move { read(&core, &sp).await }
    });
    while state.reads.load(Ordering::SeqCst) < 2 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    expire(&core, &sp);
    read(&core, &sp).await.unwrap();
    first.await.unwrap().unwrap();
    assert!(stored(&core, &sp).phase == Phase::Ready);
    assert_eq!(state.exchanges.load(Ordering::SeqCst), 1);
    server.abort();
}
#[tokio::test]
async fn replacing_owner_during_read_suppresses_the_old_response() {
    let (url, state, server) = fixture().await;
    let (core, sp) = configured(&url, false);
    read(&core, &sp).await.unwrap();
    state.mode.store(13, Ordering::SeqCst);
    let first = tokio::spawn({
        let core = core.clone();
        let sp = sp.clone();
        async move { read(&core, &sp).await }
    });
    while state.reads.load(Ordering::SeqCst) < 2 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    core.set_website_credential(&sp.site.name, "api", Some(&grant_input(false)))
        .unwrap();
    assert_eq!(first.await.unwrap().err().unwrap().public_code, "conflict");
    server.abort();
}
#[tokio::test]
async fn pre_dispatch_cooldown_keeps_the_unconsumed_grant_recoverable() {
    let (url, state, server) = fixture().await;
    let (core, sp) = configured(&url, true);
    let scope = format!("{url}|*");
    let lease = core.policy.acquire(&scope).await.unwrap();
    core.policy
        .release(&scope, &lease, 429, Some(Duration::from_secs(120)), false)
        .unwrap();
    assert_eq!(
        read(&core, &sp).await.err().unwrap().public_code,
        "rate_limited"
    );
    assert_eq!(state.exchanges.load(Ordering::SeqCst), 0);
    assert!(stored(&core, &sp).phase == Phase::VerificationPending);
    assert_eq!(
        core.policy.acquire(&scope).await.unwrap_err().public_code,
        "rate_limited"
    );
    server.abort();
}
#[tokio::test]
async fn expired_verification_candidate_refreshes_once_then_verifies() {
    let (url, state, server) = fixture().await;
    let (core, sp) = configured(&url, true);
    state.mode.store(7, Ordering::SeqCst);
    assert!(read(&core, &sp).await.is_err());
    expire(&core, &sp);
    state.mode.store(0, Ordering::SeqCst);
    read(&core, &sp).await.unwrap();
    assert_eq!(state.exchanges.load(Ordering::SeqCst), 2);
    assert!(stored(&core, &sp).phase == Phase::Ready);
    server.abort();
}

#[tokio::test]
async fn uninstalled_preflight_is_rejected_before_any_remote_work() {
    let (url, state, server) = fixture().await;
    let (core, mut sp) = configured(&url, false);
    std::fs::remove_file(core.specs_dir.join("oauth-fixture.yaml")).unwrap();
    sp.operations[0].preflight.push(
        serde_json::from_value(json!({"name":"unsupported","method":"GET","path":"/items"}))
            .unwrap(),
    );
    assert_eq!(
        read(&core, &sp).await.err().unwrap().public_code,
        "oauth_configuration_invalid"
    );
    assert_eq!(state.reads.load(Ordering::SeqCst), 0);
    assert_eq!(state.identities.load(Ordering::SeqCst), 0);
    assert_eq!(state.exchanges.load(Ordering::SeqCst), 0);
    server.abort();
}

#[test]
fn oauth_metadata_is_fail_closed_for_legacy_headers_and_duplicate_grants() {
    let sp = definition("https://fixture.example.test");
    assert!(crate::policy::validate_spec(&sp).is_ok());
    let mut legacy = sp.clone();
    legacy.auth[0].oauth_refresh = None;
    assert!(crate::policy::validate_spec(&legacy).is_err());
    let mut mixed = sp.clone();
    mixed.auth[0].value_from = "store:oauth-fixture/api".into();
    assert!(crate::policy::validate_spec(&mixed).is_err());
    let mut duplicate = sp.clone();
    let mut other = duplicate.auth[0].clone();
    other.name = "other".into();
    duplicate.auth.push(other);
    assert!(crate::policy::validate_spec(&duplicate).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn killed_exchange_owner_never_replays_after_restart() {
    let (url, state, server) = fixture().await;
    state.mode.store(15, Ordering::SeqCst);
    let (core, sp) = configured(&url, true);
    let mut child = tokio::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "runtime::oauth::tests::oauth_child_process",
            "--nocapture",
        ])
        .env("HYCLI_OAUTH_TEST_CHILD_ROOT", &core.root)
        .spawn()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while state.exchanges.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    child.kill().await.unwrap();
    child.wait().await.unwrap();
    let reopened = Runtime::open_oauth_test(&core.root).unwrap();
    assert_eq!(
        read(&reopened, &sp).await.err().unwrap().public_code,
        "oauth_recovery_required"
    );
    assert_eq!(state.exchanges.load(Ordering::SeqCst), 1);
    assert_eq!(state.reads.load(Ordering::SeqCst), 0);
    server.abort();
}

#[tokio::test]
async fn connection_refusal_preserves_grant_for_a_later_explicit_retry() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let origin = format!("http://{address}");
    let (core, sp) = configured(&origin, true);
    let previous = serde_json::to_string(&stored(&core, &sp)).unwrap();
    drop(listener);
    assert_eq!(
        read(&core, &sp)
            .await
            .err()
            .expect("expected connection failure")
            .public_code,
        "request_failed"
    );
    assert_eq!(
        serde_json::to_string(&stored(&core, &sp)).unwrap(),
        previous
    );
    let listener = tokio::net::TcpListener::bind(address).await.unwrap();
    let (_, state, server) = fixture_on(listener).await;
    read(&core, &sp).await.unwrap();
    assert_eq!(state.exchanges.load(Ordering::SeqCst), 1);
    assert_eq!(state.reads.load(Ordering::SeqCst), 1);
    assert!(matches!(stored(&core, &sp).phase, Phase::Ready));
    server.abort();
}

#[test]
fn unnamed_oauth_credentials_are_rejected_before_storage() {
    let (core, mut sp) = configured("https://example.com", false);
    let previous = serde_json::to_string(&stored(&core, &sp)).unwrap();
    sp.auth[0].name.clear();
    sp.operations[0].auth.clear();
    assert!(
        sp.validate()
            .iter()
            .any(|issue| issue.field == "auth..oauth_refresh")
    );
    assert_eq!(
        core.set_oauth_credential(&sp, &sp.auth[0], Some(&grant_input(false)))
            .unwrap_err()
            .public_code,
        "oauth_configuration_invalid"
    );
    assert_eq!(
        serde_json::to_string(&stored(&core, &sp)).unwrap(),
        previous
    );
}

#[tokio::test]
async fn oauth_replacement_streams_only_final_resource_into_one_owned_artifact() {
    use futures_util::StreamExt;
    let (url, state, server) = fixture().await;
    state.mode.store(16, Ordering::SeqCst);
    let (core, mut sp) = configured(&url, false);
    sp.operations[0].response = Some(
        serde_json::from_value(
            json!({"format":"file", "file":{"media_types":["application/pdf"]}}),
        )
        .unwrap(),
    );
    core.install(&sp).unwrap();
    let result = read(&core, &sp).await.unwrap();
    let file = result.artifact.unwrap();
    assert_eq!(state.exchanges.load(Ordering::SeqCst), 1);
    assert_eq!(state.reads.load(Ordering::SeqCst), 2);
    assert_eq!(
        file.byte_count,
        1601 * crate::artifacts::TRANSFER_CHUNK_BYTES as u64
    );
    assert_eq!(
        std::fs::read_dir(core.root.join("artifacts"))
            .unwrap()
            .count(),
        1
    );
    let opened = crate::artifacts::ArtifactStore::open(&core.root)
        .unwrap()
        .open_completed(&file.artifact_id)
        .unwrap();
    let stream = opened.into_stream();
    futures_util::pin_mut!(stream);
    let mut count = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.unwrap();
        assert!(chunk.iter().all(|b| *b == 0x61));
        count += chunk.len() as u64;
    }
    assert_eq!(count, file.byte_count);
    server.abort();
}

#[tokio::test]
async fn file_resource_mode_does_not_expand_token_response_bounds() {
    let (url, state, server) = fixture().await;
    state.mode.store(5, Ordering::SeqCst);
    let (core, mut sp) = configured(&url, true);
    sp.operations[0].response = Some(
        serde_json::from_value(
            json!({"format":"file", "file":{"media_types":["application/pdf"]}}),
        )
        .unwrap(),
    );
    core.install(&sp).unwrap();
    assert!(read(&core, &sp).await.is_err());
    assert_eq!(state.reads.load(Ordering::SeqCst), 0);
    assert!(!core.root.join("artifacts").exists());
    server.abort();
}

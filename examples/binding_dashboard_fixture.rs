//! Owned synthetic services for real production-dashboard account-binding tests.
//! No native browser-profile import or live external account is used.
use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use hycli::{runtime::Runtime, util};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let hold = Arc::new(AtomicBool::new(false));
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(tokio::sync::Notify::new());
    let requests = Arc::new(Mutex::new(Vec::<String>::new()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let website = format!("http://{}", listener.local_addr()?);
    let root = std::path::PathBuf::from(
        std::env::var_os("HYCLI_TEST_DATA").ok_or("HYCLI_TEST_DATA required")?,
    )
    .join(util::id());
    let core = Runtime::open(&root, None, None, false, true, false)?;
    let identity_hold = hold.clone();
    let identity_entered = entered.clone();
    let identity_release = release.clone();
    let read_requests = requests.clone();
    let control_core = core.clone();
    let control_hold = hold.clone();
    let control_entered = entered.clone();
    let control_release = release.clone();
    let account_id = Arc::new(Mutex::new(String::new()));
    let control_account = account_id.clone();
    let status_core = core.clone();
    let status_account = account_id.clone();
    let status_requests = requests.clone();
    let status_entered = entered.clone();
    let router = Router::new()
        .route("/account", get(move |headers:HeaderMap| { let hold=identity_hold.clone();let entered=identity_entered.clone();let release=identity_release.clone();async move {
            let person = person(&headers);
            if person == "Alpha" && hold.swap(false, Ordering::SeqCst) { entered.store(true, Ordering::SeqCst); release.notified().await; }
            if person == "Anonymous" { (StatusCode::UNAUTHORIZED, Json(json!({"error":"signed_out"}))) }
            else { (StatusCode::OK, Json(json!({"username":person}))) }
        }}))
        .route("/items", get(move |headers:HeaderMap| { let requests=read_requests.clone();async move {
            let authority=person(&headers); requests.lock().unwrap().push(authority.into()); Json(json!({"authority":authority}))
        }}))
        .route("/fixture/control", post(move |Json(body):Json<Value>| { let core=control_core.clone();let id=control_account.lock().unwrap().clone();let hold=control_hold.clone();let entered=control_entered.clone();let release=control_release.clone();async move {
            match body["action"].as_str() {
                Some("hold") => {entered.store(false,Ordering::SeqCst);hold.store(true,Ordering::SeqCst);},
                Some("reconnect") => {core.update_credential(&id,|credential| {credential.cookies[0].value="synthetic-beta".into();Ok(())}).unwrap();},
                Some("release") => release.notify_one(),
                _ => return (StatusCode::BAD_REQUEST,Json(json!({"error":"unknown_fixture_action"}))),
            }
            (StatusCode::OK,Json(json!({"ok":true})))
        }}))
        .route("/fixture/status", get(move || {let core=status_core.clone();let id=status_account.lock().unwrap().clone();let requests=status_requests.clone();let entered=status_entered.clone();async move {
            let state=core.state.read().unwrap(); let account=state.accounts.get(&id);
            Json(json!({"requests":requests.lock().unwrap().clone(),"entered":entered.load(Ordering::SeqCst),"account_id":id,"identity":account.map(|a|&a.user_id),"source_site":account.map(|a|&a.site_id),"selected":state.sites.get("named-workspace").map(|m|&m.account_id)}))
        }}));
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let spec=hycli::spec::parse(json!({"site":{"name":"named-workspace","title":"Named workspace","base_url":website},"operations":[{"name":"read-session","method":"GET","path":"/items","effect":"read","evidence":"Owned synthetic fixture","response":{"format":"json","required_pointers":["/authority"]}}]}).to_string().as_bytes())?;
    core.install(&spec)?;
    // Origin-hash ownership deliberately differs from the imported/named application ID.
    for (value, profile) in [
        ("synthetic-alpha", "Imported Alpha"),
        ("synthetic-gamma", "Other profile"),
    ] {
        let cookies = hycli::accounts::parse_cookies(
            &json!([{"name":"sid","value":value,"domain":"127.0.0.1","path":"/"}]).to_string(),
            &website,
        )?;
        let account = core.connect_account(
            "",
            &website,
            cookies,
            "File import",
            profile,
            &format!("{website}/account"),
        )?;
        core.refresh_account(&account.id).await?;
        if value == "synthetic-alpha" {
            *account_id.lock().unwrap() = account.id;
        }
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let web = hycli::dashboard::Web::new(core, port)?;
    let ticket = web.test_bootstrap_ticket();
    let server =
        tokio::spawn(async move { axum::serve(listener, hycli::dashboard::router(web)).await });
    // Test-only issuance, normal production redemption; not file-handoff coverage.
    let response = reqwest::Client::builder()
        .no_proxy()
        .build()?
        .post(format!("http://127.0.0.1:{port}/api/bootstrap"))
        .json(&json!({"ticket": ticket}))
        .send()
        .await?
        .error_for_status()?;
    let cookie = response.headers()["set-cookie"]
        .to_str()?
        .split(';')
        .next()
        .ok_or("fixture session cookie missing")?
        .to_owned();
    let session: Value = response.json().await?;
    let output = std::env::var_os("HYCLI_FIXTURE_OUTPUT").ok_or("HYCLI_FIXTURE_OUTPUT required")?;
    util::atomic_file(
        std::path::Path::new(&output),
        &serde_json::to_vec(
            &json!({"dashboard":format!("http://127.0.0.1:{port}"),"website":website,"session":{"cookie":cookie,"csrf":session["csrf"]},"account_id":account_id.lock().unwrap().clone()}),
        )?,
    )?;
    server.await??;
    Ok(())
}
fn person(headers: &HeaderMap) -> &'static str {
    let cookie = headers
        .get("cookie")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if cookie.contains("sid=synthetic-alpha") {
        "Alpha"
    } else if cookie.contains("sid=synthetic-beta") {
        "Beta"
    } else if cookie.contains("sid=synthetic-gamma") {
        "Gamma"
    } else {
        "Anonymous"
    }
}

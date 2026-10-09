//! End-to-end test service. All accounts, keys and writes are synthetic.
use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use hycli::{
    runtime::Runtime,
    state::{ProviderSettings, SiteDescription},
    util,
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let restart_dashboard = Arc::new(tokio::sync::Notify::new());
    let fail_summary = Arc::new(AtomicBool::new(false));
    let delay_ai_ms = Arc::new(AtomicU64::new(0));
    let control_restart = restart_dashboard.clone();
    let control_fail = fail_summary.clone();
    let control_delay = delay_ai_ms.clone();
    let mutations = Arc::new(AtomicUsize::new(0));
    let provider_calls = Arc::new(AtomicUsize::new(0));
    let secret_prompts = Arc::new(AtomicUsize::new(0));
    let intent_prompts = Arc::new(AtomicUsize::new(0));
    let website_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let website = format!("http://{}", website_listener.local_addr()?);
    let write_count = mutations.clone();
    let counts = mutations.clone();
    let calls = provider_calls.clone();
    let private = secret_prompts.clone();
    let intent_count = intent_prompts.clone();
    let site=Router::new().route("/",get(||async{axum::response::Html("<html><head><title>Research library</title><link rel='icon' href='/favicon.svg'></head><body><h1>Research library</h1><p>Search and save reference material.</p><a href='/docs/openapi.json'>API documentation</a></body></html>")}))
      .route("/favicon.svg",get(|headers:HeaderMap|async move {if headers.get("sec-fetch-dest").and_then(|v|v.to_str().ok()) != Some("image") { assert!(headers.get("cookie").is_none(), "Broker image fetch must be anonymous"); } ([ ("content-type", "image/svg+xml") ], r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64"><rect width="64" height="64" rx="14" fill="#294c3d"/><path d="M15 18h15v29H15zm19 0h15v29H34z" fill="#fafaf7"/></svg>"##)}))
      .route("/docs/openapi.json",get(||async{Json(json!({"openapi":"3.1.0","info":{"title":"Research library","version":"1"},"paths":{"/articles":{"get":{"operationId":"find-articles","summary":"Search articles"}},"/tasks":{"post":{"operationId":"create-task","summary":"Create a task","requestBody":{"content":{"application/json":{"schema":{"type":"object","properties":{"title":{"type":"string"}},"required":["title"]}}}}}},"/account":{"get":{"operationId":"read-account","summary":"Read the current account"}}}}))}))
      .route("/articles",get(||async{Json(json!({"items":[{"title":"Learning to observe","collection":"Research","saved":true},{"title":"A quieter workflow","collection":"Design","saved":false}],"token":"synthetic-response-secret"}))}))
      .route("/tasks",post(move|Json(body):Json<Value>|{let c=write_count.clone();async move{c.fetch_add(1,Ordering::SeqCst);Json(json!({"created":true,"title":body["title"],"id":42}))}}))
      .route("/account",get(|headers:HeaderMap|async move{if headers.get("cookie").and_then(|v|v.to_str().ok()).is_some_and(|v|v.contains("session=synthetic-browser-secret")){(StatusCode::OK,Json(json!({"user":{"id":"mina","name":"Mina Kim","email":"mina@example.com"},"csrf":"synthetic-csrf-secret"})))}else{(StatusCode::UNAUTHORIZED,Json(json!({"error":"sign in"})))}}))
      .route("/key-items",get(|headers:HeaderMap|async move {
        if headers.get("x-api-key").and_then(|v|v.to_str().ok()) == Some("synthetic-website-key") {
            (StatusCode::OK, Json(json!({"items":[{"title":"Private reference","source":"API key"}],"echo":"synthetic-website-key"})))
        } else { (StatusCode::UNAUTHORIZED, Json(json!({"error":"key required"}))) }
      }))
      .route("/fixture/control",post(move |Json(body):Json<Value>| { let restart=control_restart.clone();let fail=control_fail.clone();let delay=control_delay.clone();async move {
        if let Some(value)=body["fail_summary"].as_bool(){fail.store(value,Ordering::SeqCst);}
        if let Some(value)=body["delay_ai_ms"].as_u64(){delay.store(value.min(10000),Ordering::SeqCst);}
        if body["restart_dashboard"].as_bool()==Some(true){restart.notify_one();}
        Json(json!({"ok":true}))
      }}))
      .route("/fixture/status",get(move||{let c=counts.clone();let p=calls.clone();let s=private.clone();let intent=intent_count.clone();async move{Json(json!({"intent_prompts":intent.load(Ordering::SeqCst),"writes":c.load(Ordering::SeqCst),"provider_calls":p.load(Ordering::SeqCst),"secret_prompts":s.load(Ordering::SeqCst)}))}}));
    tokio::spawn(async move {
        axum::serve(website_listener, site).await.unwrap();
    });
    let api_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let provider_url = format!("http://{}", api_listener.local_addr()?);
    let source_url = website.clone();
    let api=Router::new().route("/models",get(||async{Json(json!({"data":[{"id":"fixture-model"}]}))})).route("/responses",post(move|Json(body):Json<Value>|{let website=source_url.clone();let calls=provider_calls.clone();let secrets=secret_prompts.clone();let intent=intent_prompts.clone();let fail=fail_summary.clone();let delay=delay_ai_ms.clone();async move{
        calls.fetch_add(1,Ordering::SeqCst);let prompt=body.pointer("/input/1/content").and_then(Value::as_str).unwrap_or("");if ["synthetic-browser-secret","synthetic-csrf-secret","synthetic-response-secret","synthetic-provider-key","synthetic-website-key"].iter().any(|s|prompt.contains(s)){secrets.fetch_add(1,Ordering::SeqCst);}
        let input:Value=serde_json::from_str(prompt).unwrap_or(Value::Null);
        if input["intended_outcome"]=="Find references about design" { intent.fetch_add(1,Ordering::SeqCst); }
        tokio::time::sleep(std::time::Duration::from_millis(delay.load(Ordering::SeqCst))).await;
        if input.get("result").is_some() && fail.load(Ordering::SeqCst) { return (StatusCode::SERVICE_UNAVAILABLE,Json(json!({"error":{"message":"Synthetic summary failure"}}))).into_response(); }
        let text=if input.get("output_contract").is_some(){if !input["sources"].as_array().is_some_and(|sources|sources.iter().any(|source|source["kind"]=="API documentation")){json!({"read_urls":[format!("{website}/docs/openapi.json")]}).to_string()}else if !input["sources"].as_array().is_some_and(|sources| sources.iter().any(|source| source["content"]["tool"]=="web.read" && source["content"]["result"]["error"]=="unsupported_evidence")) { json!({"calls":[{"tool":"web.read","arguments":{"url":format!("{website}/not-published")}}]}).to_string() } else if !input["sources"].as_array().is_some_and(|sources| sources.iter().any(|source| source["content"]["tool"]=="evidence.search" && source["content"]["result"]["source_url"]==format!("{website}/docs/openapi.json"))) { json!({"calls":[{"tool":"evidence.search","arguments":{"source_url":format!("{website}/docs/openapi.json"),"query":"/articles"}}]}).to_string() } else{let id=input["site_id"].as_str().unwrap_or("library");let mut spec=fixture_spec(id,"Research library",&website);let operation=match input["worker_role"].as_str(){Some("explore")=>"find-articles",Some("organize")=>"create-task",Some("check")=>"read-account",_=>""};if !operation.is_empty(){spec["operations"].as_array_mut().unwrap().retain(|item|item["name"]==operation);}if operation=="find-articles" && !input["sources"].as_array().is_some_and(|sources| sources.iter().any(|source|source["kind"]=="Read verification failures; revise using observed evidence")) { spec["operations"][0]["response"]=json!({"format":"json","required_pointers":["/incorrect-fixture-field"]}); }json!({"spec":spec,"description":description("Research library","Find the references you need.")}).to_string()}}else if input.get("spec").is_some(){description(input["spec"]["site"]["title"].as_str().unwrap_or("Website"),"Find the information you need.").to_string()}else if input.get("result").is_some(){"Two references are ready to use: Learning to observe and A quieter workflow.".into()}else{"OK".into()};
        Json(json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":text}]}]})).into_response()
    }}));
    let mut api = api;
    for (path, expected_key) in [
        ("/api/paas/v4/chat/completions", "synthetic-standard-key"),
        ("/api/coding/paas/v4/chat/completions", "synthetic-plan-key"),
    ] {
        api = api.route(path, post(move |headers: HeaderMap, Json(body): Json<Value>| async move {
            if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some(&format!("Bearer {expected_key}")) || body.to_string().contains(expected_key) {
                return (StatusCode::UNAUTHORIZED, Json(json!({"error":"fixture credential mismatch"})));
            }
            (StatusCode::OK, Json(json!({"choices":[{"finish_reason":"stop","message":{"content":"OK"}}]})))
        }));
    }
    tokio::spawn(async move {
        axum::serve(api_listener, api).await.unwrap();
    });
    let project = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = std::env::var_os("HYCLI_TEST_DATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| project.join(".cache/fixtures"))
        .join(util::id());
    let mut core = Runtime::open(&root, None, None, true, true, false)?;
    let origins = &mut Arc::get_mut(&mut core).unwrap().ai.origins;
    origins.insert("zai".into(), format!("{provider_url}/api/paas/v4"));
    origins.insert(
        "zai-coding-plan".into(),
        format!("{provider_url}/api/coding/paas/v4"),
    );
    origins.insert("openai".into(), provider_url);
    core.vault
        .lock()
        .unwrap()
        .set("providers", "openai", "synthetic-provider-key")?;
    core.state.update(|d| {
        d.settings.default_provider = "openai".into();
        d.providers.insert(
            "openai".into(),
            ProviderSettings {
                model: "fixture-model".into(),
                connected: true,
                models: vec!["fixture-model".into()],
                checked_at: util::now(),
            },
        );
        Ok(())
    })?;
    for (id, title, summary) in [
        (
            "library",
            "Research library",
            "Find the references you need.",
        ),
        (
            "projects",
            "Project workspace",
            "Keep your projects moving.",
        ),
        ("calendar", "Team calendar", "Make room for what is next."),
    ] {
        let sp = hycli::spec::parse(fixture_spec(id, title, &website).to_string().as_bytes())?;
        core.install(&sp)?;
        let mut desc: SiteDescription = serde_json::from_value(description(title, summary))?;
        desc.hash = Runtime::spec_hash(&sp)?;
        core.state.update(|d| {
            d.sites
                .entry(id.into())
                .or_default()
                .descriptions
                .insert("en".into(), desc);
            Ok(())
        })?;
    }
    let cookies=hycli::accounts::parse_cookies(&json!([{"name":"session","value":"synthetic-browser-secret","domain":"127.0.0.1","path":"/","httpOnly":true}]).to_string(),&website)?;
    let account = core.connect_account(
        "library",
        &website,
        cookies,
        "Chrome",
        "Personal",
        &format!("{website}/account"),
    )?;
    core.refresh_account(&account.id).await?;
    core.fill_missing_site_images().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let address = format!("http://127.0.0.1:{port}");
    util::atomic_file(
        &std::env::var_os("HYCLI_FIXTURE_OUTPUT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| project.join(".cache/e2e-ports.json")),
        &serde_json::to_vec(&json!({"dashboard":address,"website":website,"data":root}))?,
    )?;
    println!("Synthetic dashboard fixture: {address}");
    let mut first = Some(listener);
    loop {
        let listener = match first.take() {
            Some(listener) => listener,
            None => tokio::net::TcpListener::bind(("127.0.0.1", port)).await?,
        };
        let router = hycli::dashboard::router(hycli::dashboard::Web::new(core.clone(), port));
        let mut server = tokio::spawn(async move { axum::serve(listener, router).await });
        tokio::select! {
            result = &mut server => { result??; break; }
            _ = restart_dashboard.notified() => { server.abort(); let _ = server.await; }
        }
    }
    Ok(())
}
fn fixture_spec(id: &str, title: &str, website: &str) -> Value {
    json!({"spec_version":1,"site":{"name":id,"title":title,"base_url":website},"operations":[{"name":"find-articles","desc":"Find references in your library.","method":"GET","path":"/articles","effect":"read","evidence":format!("{website}/docs/openapi.json"),"query":{"q":{"type":"string","required":false}}},{"name":"read-account","desc":"Read the account currently in use.","method":"GET","path":"/account","effect":"read","evidence":format!("{website}/docs/openapi.json")},{"name":"create-task","desc":"Add a task for the next step.","method":"POST","path":"/tasks","effect":"write","evidence":format!("{website}/docs/openapi.json"),"body":{"json":{"title":{"type":"string","required":true}}}}]})
}
fn description(title: &str, summary: &str) -> Value {
    json!({"title":title,"summary":summary,"actions":{"find-articles":{"title":"Find articles","description":"Find references in your library.","output":"Articles matching your search.","inputs":{"q":{"label":"Search for","hint":"Words to find"}}},"read-account":{"title":"Read account details","description":"See which account is connected.","output":"Your account name and email.","inputs":{}},"create-task":{"title":"Create a task","description":"Add a task for the next step.","output":"The task you created.","inputs":{"title":{"label":"Task name","hint":"What needs to be done?"}}}}})
}

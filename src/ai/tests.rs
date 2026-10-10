use super::*;

#[test]
fn generated_authentication_evidence_remains_valid_json_after_redaction() {
    let proposal = json!({"calls":[{"tool":"evidence.search","arguments":{"query":"Authorization: Bot"}}],"spec":{"auth":[{"name":"account","kind":"header","header":"Authorization","value_from":"store:site/api-key"}],"operations":[{"auth":"account"}]},"note":"Bearer synthetic-private-value"});
    let clean = crate::util::redact_generated_text(&proposal.to_string());
    let decoded: Value = serde_json::from_str(&clean).unwrap();
    assert_eq!(decoded["spec"], proposal["spec"]);
    assert_eq!(
        decoded["calls"][0]["arguments"]["query"],
        "Authorization: [private]"
    );
    assert!(!clean.contains("synthetic-private-value"));
}
use axum::{Json, Router, http::HeaderMap, routing::post};

#[tokio::test]
async fn zai_plans_use_separate_routes_keys_and_checks_without_fallback() {
    use axum::http::StatusCode;
    use std::sync::atomic::{AtomicUsize, Ordering};

    assert_eq!(api_origin("zai").unwrap(), "https://api.z.ai/api/paas/v4");
    assert_eq!(
        api_origin("zai-coding-plan").unwrap(),
        "https://api.z.ai/api/coding/paas/v4"
    );
    let counts = Arc::new([AtomicUsize::new(0), AtomicUsize::new(0)]);
    let mut router = Router::new();
    for (i, (id, key)) in [
        ("zai", "synthetic-standard-key"),
        ("zai-coding-plan", "synthetic-plan-key"),
    ]
    .into_iter()
    .enumerate()
    {
        let route = format!(
            "{}/chat/completions",
            url::Url::parse(api_origin(id).unwrap()).unwrap().path()
        );
        let counts = counts.clone();
        router = router.route(&route, post(move |headers: HeaderMap, Json(body): Json<Value>| {
            let counts = counts.clone();
            async move {
                counts[i].fetch_add(1, Ordering::SeqCst);
                assert_eq!(headers.get("authorization").unwrap(), &format!("Bearer {key}"));
                assert!(!body.to_string().contains(key));
                if body["model"] == "unavailable-model" {
                    return (StatusCode::BAD_REQUEST, Json(json!({"error":{"code":"1311","message":"Model not found"}})));
                }
                assert_eq!(body["model"], if i == 0 { "glm-5.3" } else { "glm-5.3-flash" });
                (StatusCode::OK, Json(json!({"choices":[{"finish_reason":"stop","message":{"content":"OK"}}]})))
            }
        }));
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let mut client = Client::new(Arc::new(codex::Codex::new(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/provider-tests"),
    )));
    for id in ["zai", "zai-coding-plan"] {
        client.origins.insert(
            id.into(),
            format!(
                "{origin}{}",
                url::Url::parse(api_origin(id).unwrap()).unwrap().path()
            ),
        );
    }
    let standard = Config {
        provider: "zai".into(),
        model: String::new(),
        key: "synthetic-standard-key".into(),
    };
    let mut plan = Config {
        provider: "zai-coding-plan".into(),
        model: "glm-5.3-flash".into(),
        key: "synthetic-plan-key".into(),
    };
    assert_eq!(client.check(&standard).await.unwrap(), vec!["glm-5.3"]);
    assert_eq!(client.check(&plan).await.unwrap(), vec!["glm-5.3-flash"]);
    assert_eq!(counts[0].load(Ordering::SeqCst), 1);
    assert_eq!(counts[1].load(Ordering::SeqCst), 1);
    plan.model = "unavailable-model".into();
    assert!(client.check(&plan).await.is_err());
    assert_eq!(
        counts[0].load(Ordering::SeqCst),
        1,
        "Coding Plan failures must not fall back to the standard API"
    );
    assert_eq!(counts[1].load(Ordering::SeqCst), 2);
    server.abort();
}

#[tokio::test]
async fn all_api_adapters_keep_keys_in_transport_and_accept_completed_text() {
    for provider_id in ["openai", "anthropic", "xai", "zai", "zai-coding-plan"] {
        let id = provider_id.to_string();
        let route = match provider_id {
            "anthropic" => "/messages",
            "zai" | "zai-coding-plan" => "/chat/completions",
            _ => "/responses",
        };
        let router=Router::new().route(route,post(move|headers:HeaderMap,Json(body):Json<Value>|{let id=id.clone();async move{
            assert!(!body.to_string().contains("synthetic-provider-key"));
            if id=="anthropic"{assert_eq!(headers.get("x-api-key").unwrap(),"synthetic-provider-key");assert_eq!(headers.get("anthropic-version").unwrap(),"2023-06-01");Json(json!({"stop_reason":"end_turn","content":[{"type":"text","text":"OK"}]}))}
            else{assert_eq!(headers.get("authorization").unwrap(),"Bearer synthetic-provider-key");if is_zai(&id){Json(json!({"choices":[{"finish_reason":"stop","message":{"content":"OK"}}]}))}else{assert_eq!(body["store"],false);Json(json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"OK"}]}]}))}}
        }}));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let mut client = Client::new(Arc::new(codex::Codex::new(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/provider-tests"),
        )));
        client.origins.insert(provider_id.into(), url);
        let config = Config {
            provider: provider_id.into(),
            model: "fixture-model".into(),
            key: "synthetic-provider-key".into(),
        };
        assert_eq!(
            client
                .complete(&config, "Return OK", "Connection fixture")
                .await
                .unwrap(),
            "OK"
        );
        server.abort();
    }
}
#[test]
fn interrupted_or_truncated_output_is_not_presented_as_complete() {
    for (provider, value) in [
        (
            "openai",
            json!({"status":"incomplete","output":[{"type":"message","content":[{"type":"output_text","text":"partial"}]}]}),
        ),
        (
            "anthropic",
            json!({"stop_reason":"max_tokens","content":[{"type":"text","text":"partial"}]}),
        ),
        (
            "zai",
            json!({"choices":[{"finish_reason":"length","message":{"content":"partial"}}]}),
        ),
        (
            "zai-coding-plan",
            json!({"choices":[{"finish_reason":"length","message":{"content":"partial"}}]}),
        ),
    ] {
        assert!(extract_text(provider, &value).is_err());
    }
}

#[tokio::test]
async fn inference_runs_three_workers_and_queues_the_rest() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    let (a, m, c) = (active.clone(), maximum.clone(), calls.clone());
    let router = Router::new().route("/responses", post(move || {
        let (active, maximum, calls) = (a.clone(), m.clone(), c.clone());
        async move {
            let n = active.fetch_add(1, Ordering::SeqCst) + 1;
            maximum.fetch_max(n, Ordering::SeqCst); calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            active.fetch_sub(1, Ordering::SeqCst);
            Json(json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"OK"}]}]}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let mut client = Client::new(Arc::new(codex::Codex::new(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/provider-tests"),
    )));
    client.origins.insert("openai".into(), origin);
    let config = Config {
        provider: "openai".into(),
        model: "fixture-model".into(),
        key: "synthetic-key".into(),
    };
    let results = futures_util::future::join_all(
        (0..7).map(|_| client.complete(&config, "Reply OK", "Fixture")),
    )
    .await;
    assert!(
        results
            .into_iter()
            .all(|result| result.is_ok_and(|text| text == "OK"))
    );
    assert_eq!(calls.load(Ordering::SeqCst), 7);
    assert_eq!(
        maximum.load(Ordering::SeqCst),
        3,
        "Independent AI work should run together, with a hard limit of three"
    );
    task.abort();
}

#[cfg(unix)]
#[tokio::test]
async fn codex_uses_independent_workers_without_mixing_replies() {
    let root = std::env::var_os("HYCLI_TEST_DATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/tests")
        })
        .join(crate::util::id());
    let binary =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/mock_codex.py");
    let adapter = codex::Codex::with_binary(root.clone(), binary);
    let prompts: Vec<_> = (0..6).map(|i| format!("OK worker {i}")).collect();
    let replies = futures_util::future::join_all(
        prompts
            .iter()
            .map(|prompt| adapter.complete("fixture-model", "Return the supplied text.", prompt)),
    )
    .await;
    for (prompt, reply) in prompts.iter().zip(replies) {
        assert_eq!(&reply.unwrap(), prompt);
    }
    let state: Value =
        serde_json::from_slice(&std::fs::read(root.join("concurrency.json")).unwrap()).unwrap();
    assert_eq!(state["maximum"], 3);
    assert_eq!(state["calls"], 6);
    assert_eq!(state["active"], 0);
    assert!(
        adapter
            .complete(
                "fixture-model",
                "Return the supplied text.",
                "NATIVE_TOOL_ATTEMPT"
            )
            .await
            .is_err(),
        "Agent delegation must not enable unrelated native tool execution"
    );
}

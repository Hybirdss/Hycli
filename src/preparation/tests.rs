use super::{
    grounding::{apply_documented_auth, documented_auth, merge_worker_spec, supported_path},
    models::{
        WorkflowReview, complete_workflows, decode_plan, redact_self_qa, valid_understanding,
        writes_without_declared_auth,
    },
};
use crate::{
    policy,
    smoke::{self, ReadCheck},
    spec::Spec,
    state::WebsiteUnderstanding,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn writes_without_a_credentialed_auth_strategy_are_reported_as_gaps() {
    let public = crate::spec::parse(
        json!({"spec_version":2,"site":{"name":"s","base_url":"https://x.example"},"operations":[
            {"name":"read-items","method":"GET","path":"/items","effect":"read","evidence":"fixture"},
            {"name":"create-item","method":"POST","path":"/items","effect":"write","evidence":"fixture"}
        ]})
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    assert_eq!(
        writes_without_declared_auth(&public),
        vec!["create-item".to_string()]
    );

    let authed = crate::spec::parse(
        json!({"spec_version":2,"site":{"name":"s","base_url":"https://x.example"},"auth":[{"name":"session","kind":"browser"}],"operations":[
            {"name":"create-item","method":"POST","path":"/items","auth":"session","effect":"write","evidence":"fixture"}
        ]})
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    assert!(writes_without_declared_auth(&authed).is_empty());
}

#[test]
fn nullable_optional_review_sections_do_not_discard_observed_coverage() {
    let plan = decode_plan(r#"{"workflow_review":{"decision":"continue","next_steps":null,"read_checks":null,"workflows":[{"id":"read","steps":[{"step":1,"actions":["find"],"input_sources":null,"result_check":"Records returned","blocker":null,"attempts":null}]}]}}"#).unwrap();
    let review = plan.workflow_review.unwrap();
    assert!(review.read_checks.is_empty());
    let typed = decode_plan(r#"{"workflow_review":{"read_checks":[{"action":"find","inputs":{"page":2,"active":true,"filter":{"tag":"design"}}}]}}"#).unwrap().workflow_review.unwrap();
    assert_eq!(typed.read_checks[0].inputs["page"], "2");
    assert_eq!(typed.read_checks[0].inputs["active"], "true");
    assert_eq!(typed.read_checks[0].inputs["filter"], r#"{"tag":"design"}"#);
    assert!(review.workflows[0].steps[0].blocker.is_empty());
    assert_eq!(
        review.workflows[0].steps[0].result_check,
        "Records returned"
    );
}

#[test]
fn workflow_read_inputs_resolve_previous_results_without_guessing_identifiers() {
    let results = BTreeMap::from([("find".into(), json!({"items":[{"id":"private-record-id"}]}))]);
    let check = ReadCheck {
        action: "read".into(),
        inputs: BTreeMap::from([
            ("id".into(), "$result:find:/items/0/id".into()),
            ("format".into(), "full".into()),
        ]),
    };
    let inputs = smoke::resolve(&check, &results).unwrap();
    assert_eq!(inputs["id"], "private-record-id");
    assert_eq!(inputs["format"], "full");
    assert!(smoke::resolve(&check, &BTreeMap::new()).is_none());
    let mut malformed = check.clone();
    malformed
        .inputs
        .insert("id".into(), "$result:find:/items/99/id".into());
    assert!(smoke::resolve(&malformed, &results).is_none());
    malformed
        .inputs
        .insert("id".into(), "$result:find:/items".into());
    assert!(smoke::resolve(&malformed, &results).is_none());
}

#[test]
fn workflow_coverage_rejects_missing_prerequisites_inputs_and_duplicate_steps() {
    let plan: WebsiteUnderstanding = serde_json::from_value(json!({"summary":"Find a record and update it", "workflows":[{"id":"edit","title":"Edit a record","benefit":"Keep a record current","steps":["Find its identifier","Update its content"],"success_criteria":"Updated record returned"}]})).unwrap();
    assert!(valid_understanding(&plan));
    let spec = crate::spec::parse(json!({"spec_version":1,"site":{"name":"fixture","base_url":"https://example.com"},"operations":[{"name":"find","method":"GET","path":"/records","effect":"read"},{"name":"edit","method":"POST","path":"/records/{id}","effect":"write","params":{"id":{"type":"string","required":true}},"body":{"json":{"content":{"type":"string","required":true}}}}]}).to_string().as_bytes()).unwrap();
    let mut review: WorkflowReview = serde_json::from_value(json!({"decision":"complete","workflows":[{"id":"edit","steps":[{"step":1,"actions":["find"],"result_check":"Records contain identifiers"},{"step":2,"actions":["edit"],"input_sources":{"edit.id":"Identifier returned by find","edit.content":"Content supplied by the user"},"result_check":"Updated record returned"}]}]})).unwrap();
    assert_eq!(complete_workflows(&plan, &review, &spec), 1);
    let original = review.clone();
    review.workflows[0].steps.remove(0);
    assert_eq!(complete_workflows(&plan, &review, &spec), 0);
    review = original.clone();
    review.workflows[0].steps[1].input_sources.remove("edit.id");
    assert_eq!(complete_workflows(&plan, &review, &spec), 0);
    review = original.clone();
    review.workflows[0].steps[1].step = 1;
    assert_eq!(complete_workflows(&plan, &review, &spec), 0);
    review = original.clone();
    review.workflows[0].steps[1].actions = vec!["invented".into()];
    assert_eq!(complete_workflows(&plan, &review, &spec), 0);
    review = original;
    review.workflows[0].steps[1].blocker = "No authenticated session".into();
    assert_eq!(complete_workflows(&plan, &review, &spec), 0);
}

#[test]
fn web_and_api_workers_keep_destinations_credentials_and_colliding_names() {
    let mut api = crate::spec::parse(json!({"spec_version":1,"site":{"name":"fixture","base_url":"https://api.example.com","source_url":"https://example.com"},"auth":[{"name":"key","kind":"header","header":"Authorization","value_from":"store:fixture/api-key"}],"operations":[{"name":"current","method":"GET","path":"/user","auth":"key","effect":"read","evidence":"published API"}]}).to_string().as_bytes()).unwrap();
    let web = crate::spec::parse(json!({"spec_version":1,"site":{"name":"fixture","base_url":"https://example.com","source_url":"https://example.com"},"auth":[{"name":"browser","kind":"browser"}],"operations":[{"name":"current","method":"GET","path":"/account","auth":"browser","effect":"read","evidence":"verified browser read"}]}).to_string().as_bytes()).unwrap();
    let names = merge_worker_spec(&mut api, web, "check").unwrap();
    assert_eq!(api.operations.len(), 2);
    let browser = api.op(&names["current"]).unwrap();
    assert_ne!(browser.name, "current");
    assert_eq!(api.operation_base(browser), "https://example.com");
    assert_eq!(
        api.find_auth("key").unwrap().origin,
        "https://api.example.com"
    );
    assert_eq!(
        api.build_request(browser, &Default::default()).unwrap().0,
        "https://example.com/account"
    );
    policy::validate_spec(&api).unwrap();
    let mut wrong = api;
    wrong.operations[0].base_url = "https://example.com".into();
    assert_eq!(
        policy::validate_spec(&wrong).err().unwrap().public_code,
        "unsafe_target"
    );
}

#[test]
fn proven_browser_read_replaces_provisional_api_key_action() {
    let base = json!({"spec_version":1,"site":{"name":"fixture","base_url":"https://example.com"},"auth":[{"name":"key","kind":"header","header":"Authorization","value_from":"store:fixture/api-key"}],"operations":[{"name":"current","method":"GET","path":"/current","auth":"key","effect":"read","evidence":"documented"}]});
    let mut original = crate::spec::parse(base.to_string().as_bytes()).unwrap();
    let mut browser = base;
    browser["auth"] = json!([{"name":"browser-session","kind":"browser"}]);
    browser["operations"][0]["auth"] = json!("browser-session");
    let browser = crate::spec::parse(browser.to_string().as_bytes()).unwrap();
    assert!(
        merge_worker_spec(&mut original, browser, "check")
            .unwrap()
            .contains_key("current")
    );
    assert_eq!(original.operations.len(), 1);
    assert_eq!(original.operations[0].auth, "browser-session");
}

#[test]
fn documented_security_survives_redaction_and_cannot_be_omitted() {
    let doc = json!({"openapi":"3.1.0","components":{"securitySchemes":{"BearerAuth":{"type":"http","scheme":"bearer","example":"synthetic-secret"},"ApiKey":{"type":"apiKey","in":"header","name":"x-api-key"},"OAuth":{"type":"oauth2","client_secret":"synthetic-secret"}}},"security":[{"BearerAuth":[]}],"paths":{"/private":{"get":{}},"/key":{"get":{"security":[{"ApiKey":[]}]}},"/public":{"get":{"security":[]}},"/unsupported":{"get":{"security":[{"OAuth":[]}]}}}});
    let requirements = documented_auth(&doc, "fixture");
    assert!(requirements[&("GET".into(), "/private".into())].is_some());
    assert!(!requirements.contains_key(&("GET".into(), "/public".into())));
    assert!(requirements[&("GET".into(), "/unsupported".into())].is_none());
    let metadata = serde_json::to_string(&requirements.values().collect::<Vec<_>>()).unwrap();
    assert!(!metadata.contains("synthetic-secret"));
    let mut spec = crate::spec::parse(json!({"spec_version":1,"site":{"name":"fixture","base_url":"https://example.com"},"operations":[{"name":"private","method":"GET","path":"/private","effect":"read"},{"name":"unsupported","method":"GET","path":"/unsupported","effect":"read"}]}).to_string().as_bytes()).unwrap();
    apply_documented_auth(&mut spec, &requirements);
    assert_eq!(spec.operations.len(), 1);
    assert_eq!(
        spec.find_auth(&spec.operations[0].auth).unwrap().prefix,
        "Bearer "
    );
    policy::validate_spec(&spec).unwrap();
}

#[test]
fn worker_merge_preserves_independent_auth_models_and_headers() {
    fn worker(name: &str, path: &str, header: &str) -> Spec {
        crate::spec::parse(json!({"spec_version":1,"site":{"name":"fixture","base_url":"https://example.com"},"defaults":{"headers":{"accept":header}},"auth":[{"name":"key","kind":"header","header":"x-api-key","prefix":"","value_from":format!("store:fixture/{name}")}],"models":{"item":{"label":{"path":name,"type":"string"}}},"operations":[{"name":name,"method":"GET","path":path,"effect":"read","evidence":"documented","auth":"key","model":"item"}]}).to_string().as_bytes()).unwrap()
    }
    let mut first = worker("search", "/search", "application/json");
    let second = worker("account", "/account", "application/vnd.example+json");
    assert_eq!(
        merge_worker_spec(&mut first, second, "check").unwrap(),
        BTreeMap::from([("account".into(), "account".into())])
    );
    policy::validate_spec(&first).unwrap();
    let account = first.op("account").unwrap();
    assert!(account.auth.starts_with("check-key-"));
    assert!(account.model.starts_with("check-item-"));
    assert_eq!(
        first.find_auth(&account.auth).unwrap().value_from,
        "store:fixture/account"
    );
    assert_eq!(
        first.op("search").unwrap().headers["accept"],
        "application/json"
    );
    assert_eq!(account.headers["accept"], "application/vnd.example+json");
}

#[test]
fn worker_merge_preserves_v2_derived_auth_preflight_and_template_paths() {
    let mut first = crate::spec::parse(
        json!({
            "spec_version": 1,
            "site": {"name": "fixture", "base_url": "https://example.com"},
            "auth": [{"name": "key", "kind": "header", "header": "Authorization", "value_from": "store:fixture/api-key"}],
            "operations": [{"name": "feed", "method": "GET", "path": "/api/feed", "auth": "key", "effect": "read", "evidence": "documented"}]
        })
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    let second = crate::spec::parse(
        json!({
            "spec_version": 2,
            "site": {"name": "fixture", "base_url": "https://example.com"},
            "auth": [{"name": "xsrf", "kind": "derived", "header": "X-CSRF-Token", "value_from": "cookie:ct0"}],
            "operations": [{
                "name": "feed",
                "method": "GET",
                "path": "/api/feed",
                "auth": "xsrf",
                "effect": "read",
                "evidence": "verified session read",
                "preflight": [{"name": "csrf", "path": "/", "extract": {"token": "css:meta[name='csrf']@content"}}],
                "headers": {"X-Token": "{{step.csrf.token}}"}
            }]
        })
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    merge_worker_spec(&mut first, second, "check").unwrap();
    assert_eq!(first.spec_version, 2);
    assert_eq!(first.operations.len(), 1);
    assert_eq!(first.operations[0].auth, "xsrf");
    assert_eq!(first.operations[0].preflight.len(), 1);
    policy::validate_spec(&first).unwrap();
    let evidence = BTreeSet::from(["/_next/data/build-123/index.json".into()]);
    assert!(supported_path(
        "/_next/data/{{step.next.buildId}}/index.json",
        &evidence
    ));
}

#[test]
fn self_qa_decoding_and_redaction_preserves_adversarial_audit_trail() {
    let mut plan = decode_plan(
        r#"{
            "self_qa": [
                {
                    "lens": "exhaust_design_space",
                    "question": "Can SSR state replace fragile DOM selectors?",
                    "evidence": null,
                    "answer": "Found __NEXT_DATA__ with Bearer sk-secrettoken1234567890abcdef",
                    "action_taken": "Set ssr_json to auto"
                },
                "ignored-non-object"
            ],
            "understanding": {
                "summary": "Read structured posts",
                "self_qa": null,
                "workflows": [{
                    "id": "posts",
                    "title": "Read posts",
                    "benefit": "Inspect feed",
                    "steps": ["Fetch posts"],
                    "success_criteria": "Posts returned"
                }]
            },
            "workflow_review": {
                "decision": "continue",
                "self_qa": [{
                    "lens": "interrogate_id_chain",
                    "question": "Where does post_id originate?",
                    "evidence": "list-posts output",
                    "answer": "From /items/0/id",
                    "action_taken": "Chain $result:list-posts:/items/0/id"
                }]
            }
        }"#,
    )
    .unwrap();
    assert_eq!(plan.self_qa.len(), 1);
    redact_self_qa(&mut plan.self_qa);
    assert!(
        !plan.self_qa[0]
            .answer
            .contains("sk-secrettoken1234567890abcdef")
    );
    assert_eq!(plan.self_qa[0].action_taken, "Set ssr_json to auto");
    let review = plan.workflow_review.unwrap();
    assert_eq!(review.self_qa.len(), 1);
    assert_eq!(review.self_qa[0].lens, "interrogate_id_chain");
}

#[tokio::test]
async fn query_addressed_iframe_is_read_without_requesting_unsafe_query_links() {
    use crate::{runtime::Runtime, state::Job};
    use axum::{Router, http::Uri};
    use std::sync::{Arc, Mutex};
    for unsafe_target in [
        "/?do=delete&id=123",
        "/oauth/callback?code=opaqueAuthorizationCode&state=opaqueState",
        "/reference?sid=opaqueSessionId",
        "/reference?key=opaqueApiKey",
        "/graphql?query=mutation%20%7BdeleteAccount%7D",
        "/reference?authId=opaque",
        "/reference?accessKeyId=opaque",
    ] {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let router = Router::new().fallback(move |uri: Uri| {
            let recorded = recorded.clone();
            async move {
                recorded.lock().unwrap().push(uri.to_string());
                axum::response::Html(if uri.to_string() == "/" {
                    format!(r#"<html><body><iframe src="/EntryList.page?blogId=demo&amp;categoryNo=0"></iframe><a href="{unsafe_target}">Untrusted navigation</a></body></html>"#)
                } else {
                    "<html><body><article><h1>Public entry</h1><p>Useful blog content</p></article></body></html>".into()
                })
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let initial =
            url::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/tests")
            .join(crate::util::id());
        let core = Runtime::open(&root, None, None, false, true, false).unwrap();
        let mut job = Job::new("prepare", "en");
        job.url = initial.to_string();
        core.state.add_job(job.clone()).unwrap();
        let mut sources = vec![];
        let mut documents = BTreeMap::new();
        let mut evidence = BTreeSet::new();
        let mut links = BTreeSet::new();
        core.ingest_round_reads(
            &job,
            &initial,
            0,
            "",
            &mut vec![job.url.clone()],
            &mut BTreeSet::new(),
            &mut sources,
            &mut BTreeSet::new(),
            &mut BTreeSet::new(),
            &mut links,
            &mut BTreeSet::new(),
            &mut evidence,
            &mut BTreeSet::new(),
            &mut documents,
            &mut BTreeMap::new(),
            &mut vec![],
        )
        .await
        .unwrap();
        server.abort();
        assert_eq!(
            *requests.lock().unwrap(),
            vec!["/", "/EntryList.page?blogId=demo&categoryNo=0"],
            "unsafe link was requested: {unsafe_target}"
        );
        let frame_url = initial
            .join("/EntryList.page?blogId=demo&categoryNo=0")
            .unwrap()
            .to_string();
        assert!(links.contains(&frame_url));
        assert!(evidence.contains("/EntryList.page"));
        assert!(
            documents
                .get(&frame_url)
                .is_some_and(|text| text.contains("Useful blog content"))
        );
        assert!(sources.iter().any(|source| {
            source.content["text"]
                .as_str()
                .is_some_and(|text| text.contains("Useful blog content"))
        }));
    }
}

#[tokio::test]
async fn authenticated_document_discovery_leaves_icons_to_anonymous_image_cache() {
    use crate::{runtime::Runtime, state::Job};
    use axum::{
        Router,
        http::{HeaderMap, Uri},
    };
    use std::sync::{Arc, Mutex};
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = requests.clone();
    let router = Router::new().fallback(move |uri: Uri, headers: HeaderMap| {
        let recorded = recorded.clone();
        async move {
            recorded.lock().unwrap().push((uri.to_string(), headers.contains_key("cookie"), headers.contains_key("authorization")));
            if uri.path() == "/favicon.svg" {
                return ([("content-type", "image/svg+xml"), ("link", "")], r#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32"><rect width="32" height="32"/></svg>"#.to_owned());
            }
            ([("content-type", "text/html"), ("link", r#"</header-icon>; rel="alternate icon", </header-image>; rel=icon; title="alternate image""#)], if uri.path() == "/" {
                r#"<html><head><link rel="icon" href="/favicon.svg?version=2"><link rel="icon" href="/icon?version=2&#38;id=1"><img src="/&#105;mage"></head><body><iframe src="/EntryList.page?blogId=demo&amp;categoryNo=0"></iframe></body></html>"#.into()
            } else { "<html><article>Authenticated document</article></html>".into() })
        }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let initial = url::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let root = std::env::var_os("HYCLI_TEST_DATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/tests")
        })
        .join(crate::util::id());
    let core = Runtime::open(&root, None, None, false, true, false).unwrap();
    let cookies = crate::accounts::parse_cookies(&json!([{"name":"sid","value":"synthetic-fixture-session","domain":initial.host_str().unwrap(),"path":"/"}]).to_string(), initial.as_str()).unwrap();
    let account = core
        .connect_account("", initial.as_str(), cookies, "Test", "Test", "")
        .unwrap();
    let mut job = Job::new("prepare", "en");
    job.site_id = account.site_id.clone();
    job.url = initial.to_string();
    core.state.add_job(job.clone()).unwrap();
    let mut icons = vec![];
    let mut documents = BTreeMap::new();
    core.ingest_round_reads(
        &job,
        &initial,
        0,
        &account.id,
        &mut vec![job.url.clone()],
        &mut BTreeSet::new(),
        &mut vec![],
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut BTreeSet::new(),
        &mut documents,
        &mut BTreeMap::new(),
        &mut icons,
    )
    .await
    .unwrap();
    assert_eq!(
        *requests.lock().unwrap(),
        vec![
            ("/".into(), true, false),
            (
                "/EntryList.page?blogId=demo&categoryNo=0".into(),
                true,
                false
            )
        ],
        "document crawler requested a static resource"
    );
    assert!(
        documents
            .values()
            .any(|text| text.contains("Authenticated document"))
    );
    assert!(
        core.fetch_site_image("fixture", &initial, icons)
            .await
            .unwrap()
    );
    assert_eq!(
        requests.lock().unwrap().last().unwrap(),
        &("/favicon.svg?version=2".into(), false, false)
    );
    assert_eq!(requests.lock().unwrap().len(), 3);
    assert!(root.join("site-images/fixture.image").exists());
    server.abort();
}

#[test]
fn write_auth_diagnostics_are_per_operation_and_do_not_claim_verification() {
    let sp = crate::spec::parse(json!({"spec_version":2,"site":{"name":"s","base_url":"https://example.com"},"auth":[
        {"name":"session","kind":"browser"}, {"name":"public","kind":"none"},
        {"name":"key","kind":"header","header":"Authorization","value_from":"store:s/key"},
        {"name":"csrf","kind":"derived","header":"X-CSRF-Token","value_from":"cookie:csrf"}
    ],"operations":[
        {"name":"read-items","method":"GET","path":"/items","effect":"read","evidence":"fixture"},
        {"name":"read-private","method":"GET","path":"/private","auth":"session","effect":"read","evidence":"fixture"},
        {"name":"create-public","method":"POST","path":"/items","auth":"public","effect":"write","evidence":"fixture"},
        {"name":"create-undeclared","method":"POST","path":"/items","effect":"write","evidence":"fixture"},
        {"name":"create-session","method":"POST","path":"/items","auth":"session","effect":"write","evidence":"fixture"},
        {"name":"create-key","method":"POST","path":"/items","auth":"key","effect":"write","evidence":"fixture"},
        {"name":"create-csrf","method":"POST","path":"/items","auth":"csrf","effect":"write","evidence":"fixture"}
    ]}).to_string().as_bytes()).unwrap();
    assert!(sp.validate().is_empty());
    assert_eq!(
        writes_without_declared_auth(&sp),
        vec!["create-public".to_string(), "create-undeclared".to_string()]
    );
}

#[test]
fn preparation_teaches_explicit_pagination_without_changing_default_contract() {
    let environment = crate::environment::Environment {
        discovery_pending: false,
        os: "fixture".into(),
        architecture: "fixture".into(),
        browsers: vec![],
        profiles: vec![],
        capabilities: vec![],
    };
    let prompt = super::prompts::build_round_prompt(
        &crate::state::Job::new("prepare", "en"),
        &url::Url::parse("https://example.com").unwrap(),
        0,
        &environment,
        0,
        0,
        0,
        &BTreeMap::new(),
        &BTreeSet::new(),
        &BTreeSet::new(),
        &BTreeSet::new(),
        &BTreeSet::new(),
        &[],
        &[],
        &BTreeMap::new(),
        &BTreeSet::new(),
        None,
        None,
        None,
        None,
    );
    assert_eq!(prompt["output_contract"]["spec"]["spec_version"], 2);
    assert!(
        prompt["output_contract"]["spec"]["operations"][0]
            .get("pagination")
            .is_none()
    );
    let optional = &prompt["optional_pagination_contract"];
    assert_eq!(optional["spec_version"], 3);
    assert_eq!(optional["operation"]["pagination"]["input"], "cursor");
    let text = optional.to_string();
    for required in [
        "evidence",
        "15 minutes",
        "next_cursor",
        "Never automatically",
        "8 characters",
        "4096 bytes",
        "preflight",
    ] {
        assert!(text.contains(required), "{required}");
    }
    for (role, _) in super::prompts::WORKERS {
        let worker = super::prompts::build_worker_task(&prompt, role, "fixture", &BTreeMap::new());
        assert_eq!(worker["optional_pagination_contract"], *optional);
    }
}

#[test]
fn end_user_agent_guides_remain_byte_identical() {
    let canonical = include_str!("../../agent/AGENTS.md");
    assert_eq!(canonical, include_str!("../../agent/CLAUDE.md"));
    assert_eq!(
        canonical,
        include_str!("../../skills/hycli/references/agent-guide.md")
    );
}

#[test]
fn file_guidance_teaches_scoped_metadata_explanations_and_safe_reuse() {
    for guide in [
        include_str!("../../agent/AGENTS.md"),
        include_str!("../../skills/hycli/SKILL.md"),
    ] {
        for required in [
            "automatically show",
            "hycli jobs ls",
            "hycli jobs show ID",
            "hycli_activity",
            "hycli_job",
            "hycli_result",
            "metadata-based",
            "file-shaped JSON",
            "artifact_id",
            "local_path",
            "media_type",
            "byte_count",
            "created_at",
            "provenance",
            "completion",
            "availability",
            "inspection",
            "exhaustive permanent catalogue",
            "clearing or pruning",
            "integrity only",
            "embedded instructions",
            "HEAD/preflight",
            "download started",
        ] {
            assert!(
                guide.contains(required),
                "Missing file guidance: {required}"
            );
        }
        assert!(!guide.contains("8 MiB"));
        assert!(!guide.contains("100 MiB"));
    }
}

#[test]
fn invalid_worker_plan_is_not_reported_as_provider_outage() {
    let error = decode_plan(r#"{"calls":"private-invalid-plan"}"#)
        .err()
        .unwrap();
    assert_eq!(error.public_code, "ai_response_invalid");
    assert!(!format!("{error:?}").contains("private-invalid-plan"));
}

#[tokio::test]
async fn worker_failure_reasons_survive_job_reload_without_provider_content() {
    use axum::{Json, Router, http::StatusCode, routing::post};
    use std::sync::Arc;
    let router = Router::new().route("/messages", post(|Json(body): Json<serde_json::Value>| async move {
        let prompt = body["messages"][0]["content"].as_str().unwrap_or_default();
        if prompt.contains("explore") {
            (StatusCode::OK, Json(json!({"stop_reason":"max_tokens","content":[{"type":"text","text":"private-output"}]})))
        } else if prompt.contains("organize") {
            (StatusCode::OK, Json(json!({"stop_reason":"end_turn","content":[{"type":"text","text":"private-invalid-json"}]})))
        } else {
            (StatusCode::TOO_MANY_REQUESTS, Json(json!({"error":{"type":"rate_limit_error","message":"private-provider-message"}})))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".cache/provider-failure-tests")
        .join(crate::util::id());
    let mut core = crate::runtime::Runtime::open(&root, None, None, false, true, false).unwrap();
    Arc::get_mut(&mut core)
        .unwrap()
        .ai
        .origins
        .insert("anthropic".into(), origin);
    let mut job = crate::state::Job::new("prepare", "en");
    job.status = "running".into();
    core.state.add_job(job.clone()).unwrap();
    let config = crate::ai::Config {
        provider: "anthropic".into(),
        model: "fixture".into(),
        key: "synthetic-private-key".into(),
    };
    let roles = ["explore", "organize", "check"];
    let results = futures_util::future::join_all(
        roles
            .iter()
            .map(|role| core.plan_worker(&job, &config, role, json!({"fixture":role}))),
    )
    .await;
    assert!(results.iter().all(Result::is_err));
    drop(core);
    let reopened = crate::runtime::Runtime::open(&root, None, None, false, true, false).unwrap();
    for saved in [
        reopened.state.job(&job.id).unwrap(),
        reopened
            .state
            .read()
            .unwrap()
            .jobs
            .into_iter()
            .find(|j| j.id == job.id)
            .unwrap(),
    ] {
        for (role, code) in [
            ("explore", "ai_response_incomplete"),
            ("organize", "ai_response_invalid"),
            ("check", "rate_limited"),
        ] {
            assert!(
                saved
                    .notes
                    .iter()
                    .any(|n| n.agent == role && n.code == format!("agent_failed_{code}")),
                "missing {role}/{code}"
            );
        }
        let encoded = serde_json::to_string(&saved).unwrap();
        assert!(!encoded.contains("private-"));
    }
    server.abort();
}

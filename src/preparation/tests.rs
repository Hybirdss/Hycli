use super::{
    grounding::{apply_documented_auth, documented_auth, merge_worker_spec, supported_path},
    models::{
        WorkflowReview, complete_workflows, decode_plan, redact_self_qa, valid_understanding,
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

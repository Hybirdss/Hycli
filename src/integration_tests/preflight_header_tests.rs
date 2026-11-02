use super::harness::*;

fn preflight_header_spec(base: &str) -> spec::Spec {
    spec::parse(
        json!({
            "spec_version": 2,
            "site": {"name": "fixture", "base_url": base},
            "operations": [{
                "name": "read-items", "method": "GET", "path": "/items",
                "effect": "read", "evidence": "Synthetic local preflight read",
                "preflight": [{"name": "session", "path": "/session", "extract": {
                    "accessToken": "json:/accessToken",
                    "access.token": "json:/accessToken"
                }}],
                "response": {"format": "json"}
            }]
        })
        .to_string()
        .as_bytes(),
    )
    .unwrap()
}

#[test]
fn validates_existing_step_headers_and_standard_authorization_bearer() {
    let accepted = [
        ("authorization", "{{step.session.accessToken}}"),
        ("authorization", "Bearer {{step.session.accessToken}}"),
        ("Authorization", "bearer {{step.session.accessToken}}"),
        ("AUTHORIZATION", "bEaReR {{step.session.accessToken}}"),
        ("authorization", " Bearer {{ step.session.accessToken }} "),
        ("authorization", "{{step.session.access.token}}"),
        ("authorization", "Bearer {{step.session.access.token}}"),
        ("authorization", "{{ step.session.accessToken }}"),
        ("x-csrf-token", "{{step.session.accessToken}}"),
    ];
    for defaults in [false, true] {
        for (name, value) in accepted {
            let mut sp = preflight_header_spec("http://127.0.0.1:32123");
            let headers = if defaults {
                &mut sp.defaults.headers
            } else {
                &mut sp.operations[0].headers
            };
            headers.insert(name.into(), value.into());
            assert!(
                policy::validate_spec(&sp).is_ok(),
                "{name} {value}, defaults={defaults}"
            );
        }
        for (name, value) in [
            ("authorization", "Bearer synthetic-static-value"),
            ("authorization", "Basic {{step.session.accessToken}}"),
            ("x-csrf-token", "Bearer {{step.session.accessToken}}"),
            ("proxy-authorization", "{{step.session.accessToken}}"),
            ("authorization", "Bearer {{input}}"),
        ] {
            let mut sp = preflight_header_spec("http://127.0.0.1:32123");
            let headers = if defaults {
                &mut sp.defaults.headers
            } else {
                &mut sp.operations[0].headers
            };
            headers.insert(name.into(), value.into());
            assert_eq!(
                policy::validate_spec(&sp).unwrap_err().public_code,
                "data_invalid",
                "{name} {value}, defaults={defaults}"
            );
        }
    }
}

#[test]
fn step_header_lookup_preserves_field_case_dots_and_inner_whitespace() {
    for field in ["accessToken", "access.token", " accessToken"] {
        let key = format!("step.session.{field}");
        let template = format!("Bearer {{{{ {key} }}}}");
        let mut sp = preflight_header_spec("http://127.0.0.1:32123");
        sp.operations[0].preflight[0]
            .extract
            .insert(field.into(), "json:/accessToken".into());
        sp.operations[0]
            .headers
            .insert("Authorization".into(), template.clone());
        policy::validate_spec(&sp).unwrap();
        assert_eq!(
            spec::interpolate_template_str(
                &template,
                &BTreeMap::new(),
                &BTreeMap::from([(key, "synthetic-field-value".into())]),
            ),
            "Bearer synthetic-field-value"
        );
    }
}

#[tokio::test]
async fn preflight_bearer_reaches_local_service_without_overriding_account_or_leaking_output() {
    let observed = Arc::new(AtomicUsize::new(0));
    let seen = observed.clone();
    let (base, server) = host(
        Router::new()
            .route(
                "/session",
                get(|| async { Json(json!({"accessToken": "synthetic-preflight-token"})) }),
            )
            .route(
                "/items",
                get(move |headers: HeaderMap| {
                    let seen = seen.clone();
                    async move {
                        let expected = if seen.load(Ordering::SeqCst) == 0 {
                            "Bearer synthetic-preflight-token"
                        } else {
                            "Bearer synthetic-account-token"
                        };
                        let correct = headers.get_all("authorization").iter().count() == 1
                            && headers.get("authorization").and_then(|h| h.to_str().ok())
                                == Some(expected);
                        if correct {
                            seen.fetch_add(1, Ordering::SeqCst);
                        }
                        Json(json!({
                            "accepted": correct,
                            "id": "public-item-7",
                            "echo_preflight": "synthetic-preflight-token",
                            "echo_account": if expected.contains("account") {
                                "synthetic-account-token"
                            } else {
                                "not-used"
                            }
                        }))
                    }
                }),
            ),
    )
    .await;
    let core = runtime();
    let mut sp = preflight_header_spec(&base);
    sp.operations[0].headers.insert(
        "authorization".into(),
        "Bearer {{step.session.accessToken}}".into(),
    );
    assert!(
        policy::validate_spec(&sp).is_ok(),
        "standard preflight Authorization Bearer declaration must be admitted"
    );
    core.install(&sp).unwrap();
    let first = core
        .run_op_for_account(&sp, "read-items", &BTreeMap::new(), Some(""))
        .await
        .unwrap()
        .0;
    assert_eq!(first.status, 200);
    let first_body = first.body.as_ref().unwrap();
    assert_eq!(first_body["accepted"], true);
    assert_eq!(first_body["id"], "public-item-7");
    assert_eq!(first_body["echo_preflight"], "[private]");
    assert!(
        !serde_json::to_string(&first)
            .unwrap()
            .contains("synthetic-preflight-token")
    );
    assert_eq!(observed.load(Ordering::SeqCst), 1);

    let credential: crate::accounts::Credential = serde_json::from_value(json!({
        "origin": base,
        "cookies": [],
        "headers": {"authorization": "Bearer synthetic-account-token"}
    }))
    .unwrap();
    let account = core
        .register_browser_credential("fixture", credential, "Fixture", "Synthetic")
        .unwrap();
    let second = core
        .run_op_for_account(&sp, "read-items", &BTreeMap::new(), Some(&account.id))
        .await
        .unwrap()
        .0;
    assert_eq!(second.status, 200);
    let second_body = second.body.as_ref().unwrap();
    assert_eq!(second_body["accepted"], true);
    assert_eq!(second_body["id"], "public-item-7");
    assert_eq!(second_body["echo_preflight"], "[private]");
    assert_eq!(second_body["echo_account"], "[private]");
    let serialized = serde_json::to_string(&second).unwrap();
    assert!(!serialized.contains("synthetic-preflight-token"));
    assert!(!serialized.contains("synthetic-account-token"));
    assert_eq!(observed.load(Ordering::SeqCst), 2);
    server.abort();
}

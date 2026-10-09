use super::*;

fn open_test() -> Kb {
    let dir = std::env::temp_dir().join(format!(
        "hycli-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    Kb::open(&dir.join("kb.db")).unwrap()
}

#[test]
fn open_idempotent() {
    let kb = open_test();
    assert_eq!(kb.schema_version(), 1);
}

#[test]
fn ensure_node_stable() {
    let kb = open_test();
    let id1 = kb
        .ensure_node("s", "endpoint", "GET /a", &serde_json::json!({"x": 1}))
        .unwrap();
    let id2 = kb
        .ensure_node("s", "endpoint", "GET /a", &serde_json::json!({"x": 2}))
        .unwrap();
    assert_eq!(id1, id2);
    let n = kb.node("s", "endpoint", "GET /a").unwrap().unwrap();
    assert!(n.props.contains("\"x\":2") || n.props.contains("\"x\": 2"));
}

#[test]
fn gap_next_flow() {
    let kb = open_test();
    kb.add_gap("s", "r1", "ep1", "낮음", 0.4).unwrap();
    let id2 = kb.add_gap("s", "r2", "ep2", "높음", 0.9).unwrap();
    let g = kb.next("s").unwrap().unwrap();
    assert_eq!(g.subject, "ep2");
    kb.record_attempt(&AttemptInput {
        site: "s".into(),
        stage: "access".into(),
        action: "probe".into(),
        verdict: "pass".into(),
        target_kind: "endpoint".into(),
        target_key: "ep2".into(),
        status: 200,
        ..Default::default()
    })
    .unwrap();
    kb.set_gap_state(id2, "done").unwrap();
    let g = kb.next("s").unwrap().unwrap();
    assert_eq!(g.subject, "ep1");
}

#[test]
fn finding_fts_and_evidence() {
    let kb = open_test();
    let aid = kb
        .record_attempt(&AttemptInput {
            site: "s".into(),
            action: "probe".into(),
            verdict: "blocked".into(),
            ..Default::default()
        })
        .unwrap();
    let fid = kb
        .add_finding("s", "POST /api/x는 Referer 없으면 403", 0.9, &[aid])
        .unwrap();
    let (nodes, edges) = kb.subgraph(fid, 1).unwrap();
    assert!(nodes.len() >= 2 && !edges.is_empty());
    let got = kb.search_findings("s", "Referer", 10).unwrap();
    assert_eq!(got.len(), 1);
    let got = kb.search_findings("s", "존재하지않는말", 10).unwrap();
    assert!(got.is_empty());
}

#[test]
fn fsck_catches() {
    let kb = open_test();
    kb.ensure_node("s", "endpoint", "GET /lonely", &serde_json::Value::Null)
        .unwrap();
    let aid = kb
        .record_attempt(&AttemptInput {
            site: "s".into(),
            action: "probe".into(),
            verdict: "pass".into(),
            ..Default::default()
        })
        .unwrap();
    kb.add_finding("s", "증거 없는 교훈", 0.8, &[]).unwrap();
    kb.add_finding("s", "증거 있는 교훈", 0.9, &[aid]).unwrap();
    let issues = kb.fsck("s").unwrap();
    let kinds: Vec<&str> = issues.iter().map(|i| i.kind.as_str()).collect();
    assert!(kinds.contains(&"attempt-less-target"));
    assert!(kinds.contains(&"evidence-less-finding"));
}

#[test]
fn stats_aggregate() {
    let kb = open_test();
    kb.record_attempt(&AttemptInput {
        site: "s".into(),
        action: "probe".into(),
        verdict: "pass".into(),
        defense: "none".into(),
        ..Default::default()
    })
    .unwrap();
    kb.record_attempt(&AttemptInput {
        site: "s".into(),
        action: "probe".into(),
        verdict: "blocked".into(),
        defense: "cloudflare".into(),
        ..Default::default()
    })
    .unwrap();
    let ss = kb.stats("s").unwrap();
    assert_eq!(ss.len(), 2);
}

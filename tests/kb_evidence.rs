use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("hycli-kb-evidence-{}", hycli::util::id()));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hycli"))
        .args(args)
        .env("HYCLI_DATA_DIR", root)
        .env("HYCLI_TEST_DATA", root)
        .env("HYCLI_SPECS", root.join("specs"))
        .env("HYCLI_KB", root.join("kb.db"))
        .output()
        .unwrap()
}

fn ok(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn record(root: &Path) -> i64 {
    ok(cli(
        root,
        &[
            "kb",
            "record",
            "--action",
            "synthetic",
            "--verdict",
            "observed",
        ],
    ))["id"]
        .as_i64()
        .unwrap()
}

fn assert_usage_error(output: Output) {
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    assert!(diagnostic.contains("--evidence"), "{diagnostic}");
}

#[test]
fn malformed_evidence_does_not_create_a_finding() {
    for input in ["typo", "1,typo", "9223372036854775808", "1,,1", "1,", ",1"] {
        let root = root();
        assert_eq!(record(&root), 1);
        assert_usage_error(cli(
            &root,
            &[
                "kb",
                "finding-add",
                "synthetic finding",
                "--evidence",
                input,
            ],
        ));
        assert_eq!(ok(cli(&root, &["kb", "finding-search"])), json!([]));
    }
}

#[test]
fn malformed_evidence_does_not_update_an_existing_finding() {
    let root = root();
    let evidence = record(&root).to_string();
    ok(cli(
        &root,
        &[
            "kb",
            "finding-add",
            "synthetic finding",
            "--confidence",
            "0.6",
            "--evidence",
            &evidence,
        ],
    ));
    let before = ok(cli(&root, &["kb", "finding-search"]));
    assert_usage_error(cli(
        &root,
        &[
            "kb",
            "finding-add",
            "synthetic finding",
            "--confidence",
            "0.9",
            "--evidence",
            "typo",
        ],
    ));
    assert_eq!(ok(cli(&root, &["kb", "finding-search"])), before);
}

#[test]
fn evidence_ids_with_whitespace_are_preserved_and_linked() {
    let root = root();
    let first = record(&root);
    let second = record(&root);
    let evidence = format!(" {first} , {second} ");
    let finding = ok(cli(
        &root,
        &[
            "kb",
            "finding-add",
            "synthetic finding",
            "--evidence",
            &evidence,
        ],
    ));
    assert_eq!(finding["evidence"], json!([first, second]));
    let id = finding["id"].as_i64().unwrap();
    let graph = ok(cli(&root, &["kb", "graph", &id.to_string()]));
    let mut linked: Vec<_> = graph["edges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|edge| edge["src"] == id && edge["rel"] == "evidenced_by")
        .map(|edge| edge["dst"].as_i64().unwrap())
        .collect();
    linked.sort();
    assert_eq!(linked, vec![first, second]);
}

#[test]
fn omitted_and_blank_evidence_keep_the_empty_list_contract() {
    for evidence in [None, Some(""), Some("  ")] {
        let root = root();
        let mut args = vec!["kb", "finding-add", "synthetic finding"];
        if let Some(value) = evidence {
            args.extend(["--evidence", value]);
        }
        let finding = ok(cli(&root, &args));
        assert_eq!(finding["evidence"], json!([]));
        assert_eq!(
            ok(cli(&root, &["kb", "finding-search"]))
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
}

use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("hycli-kb-gap-{}", hycli::util::id()));
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

fn add_gap(root: &Path) -> i64 {
    let kb = hycli::kb::Kb::open(&root.join("kb.db")).unwrap();
    kb.add_gap("default", "synthetic", "item", "Needs a check", 0.5)
        .unwrap()
}

fn assert_missing_gap(output: Output) {
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    assert!(diagnostic.to_lowercase().contains("gap"), "{diagnostic}");
    assert!(diagnostic.contains("kb gaps"), "{diagnostic}");
}

#[test]
fn missing_gap_is_not_confirmed_in_an_empty_knowledge_base() {
    let root = root();
    assert_missing_gap(cli(&root, &["kb", "gap-set", "999", "done"]));
    assert_eq!(ok(cli(&root, &["kb", "gaps", "--state", ""])), json!([]));
}

#[test]
fn missing_gap_does_not_change_an_existing_gap() {
    let root = root();
    let id = add_gap(&root);
    let before = ok(cli(&root, &["kb", "gaps", "--state", ""]));
    assert_missing_gap(cli(
        &root,
        &["kb", "gap-set", &(id + 1).to_string(), "done"],
    ));
    assert_eq!(ok(cli(&root, &["kb", "gaps", "--state", ""])), before);
}

#[test]
fn existing_gap_updates_and_repeated_updates_are_confirmed() {
    let root = root();
    let id = add_gap(&root);
    let id_arg = id.to_string();
    assert_eq!(ok(cli(&root, &["kb", "gaps"]))[0]["state"], "open");
    for _ in 0..2 {
        assert_eq!(
            ok(cli(&root, &["kb", "gap-set", &id_arg, "done"])),
            json!({"id": id, "state": "done"})
        );
        assert_eq!(ok(cli(&root, &["kb", "gaps"])), json!([]));
        let done = ok(cli(&root, &["kb", "gaps", "--state", "done"]));
        assert_eq!(done.as_array().unwrap().len(), 1);
        assert_eq!(done[0]["id"], id);
        assert_eq!(done[0]["state"], "done");
    }
}

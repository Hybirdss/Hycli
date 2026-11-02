use hycli::{runtime::Runtime, state::StateStore, util};
use serde_json::json;
use std::path::PathBuf;

fn root() -> PathBuf {
    std::env::var_os("HYCLI_TEST_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/tests"))
        .join(util::id())
}

fn set_discovery(store: &StateStore, enabled: bool) {
    store
        .update(|data| {
            let mut value = serde_json::to_value(&data.settings)?;
            value["native_browser_discovery"] = json!(enabled);
            data.settings = serde_json::from_value(value)?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn native_discovery_policy_migrates_old_settings_without_enabling_macos() {
    let root = root();
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("state.json");
    let connection = rusqlite::Connection::open(path.with_extension("db")).unwrap();
    connection.execute_batch("CREATE TABLE settings (id INTEGER PRIMARY KEY, locale TEXT NOT NULL, theme TEXT NOT NULL, reduce_motion INTEGER NOT NULL, notifications INTEGER NOT NULL, default_provider TEXT NOT NULL, engine_path TEXT NOT NULL); INSERT INTO settings VALUES (1, 'ko', 'dark', 0, 0, '', '');").unwrap();
    drop(connection);
    let store = StateStore::open(&path).unwrap();
    let value = serde_json::to_value(store.settings().unwrap()).unwrap();
    assert_eq!(
        value["native_browser_discovery"],
        json!(!cfg!(target_os = "macos"))
    );
    assert_eq!(value["locale"], "ko");
    assert_eq!(value["theme"], "dark");
}

#[test]
fn concurrent_legacy_settings_migration_preserves_native_discovery_default() {
    let root = root();
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("state.json");
    let connection = rusqlite::Connection::open(path.with_extension("db")).unwrap();
    connection.execute_batch("CREATE TABLE settings (id INTEGER PRIMARY KEY, locale TEXT NOT NULL, theme TEXT NOT NULL, reduce_motion INTEGER NOT NULL, notifications INTEGER NOT NULL, default_provider TEXT NOT NULL, engine_path TEXT NOT NULL); INSERT INTO settings VALUES (1, 'ko', 'dark', 0, 0, '', '');").unwrap();
    drop(connection);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let store = StateStore::open(&path).unwrap();
                let settings = store.settings().unwrap();
                assert_eq!(
                    settings.native_browser_discovery,
                    !cfg!(target_os = "macos")
                );
                assert_eq!(settings.locale, "ko");
                assert_eq!(settings.theme, "dark");
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(
        StateStore::open(&path)
            .unwrap()
            .settings()
            .unwrap()
            .native_browser_discovery,
        !cfg!(target_os = "macos")
    );
}

#[test]
fn native_discovery_policy_is_visible_to_already_open_process_connections() {
    let path = root().join("state.json");
    let first = StateStore::open(&path).unwrap();
    let second = StateStore::open(&path).unwrap();
    for enabled in [false, true, false] {
        set_discovery(&first, enabled);
        assert_eq!(
            serde_json::to_value(second.settings().unwrap()).unwrap()["native_browser_discovery"],
            json!(enabled)
        );
    }
}

#[tokio::test]
async fn disabled_native_discovery_rejects_before_scan_and_preserves_accounts() {
    let core = Runtime::open(&root(), None, None, false, true, false).unwrap();
    core.state
        .update(|data| {
            data.accounts.insert(
                "existing-account".into(),
                hycli::accounts::AccountView {
                    id: "existing-account".into(),
                    site_id: "fixture".into(),
                    site_url: "http://127.0.0.1/".into(),
                    status: "connected".into(),
                    verified_identity: true,
                    ..Default::default()
                },
            );
            Ok(())
        })
        .unwrap();
    set_discovery(&core.state, false);
    let before = serde_json::to_value(core.state.read().unwrap()).unwrap();
    let result = core
        .find_browser_accounts("", "http://127.0.0.1/", "")
        .await;
    match result {
        Err(error) => assert_eq!(error.public_code, "native_discovery_disabled"),
        Ok(_) => panic!("Disabled native discovery must reject before native work"),
    }
    assert_eq!(
        before,
        serde_json::to_value(core.state.read().unwrap()).unwrap()
    );
    set_discovery(&core.state, true);
    let result = core
        .find_browser_accounts("", "http://127.0.0.1/", "")
        .await
        .unwrap();
    assert_eq!(result.profiles_checked, 0);
}

#[test]
fn native_discovery_defaults_apply_to_fresh_and_legacy_json_settings() {
    let fresh = StateStore::open(&root().join("state.json")).unwrap();
    assert_eq!(
        fresh.settings().unwrap().native_browser_discovery,
        !cfg!(target_os = "macos")
    );
    let legacy: hycli::state::Settings = serde_json::from_value(json!({"locale":"ko"})).unwrap();
    assert_eq!(legacy.native_browser_discovery, !cfg!(target_os = "macos"));
    assert_eq!(legacy.locale, "ko");
}

#[test]
fn state_startup_waits_for_database_bootstrap_owner() {
    use std::sync::mpsc;
    use std::time::Duration;
    let path = root().join("state.json");
    let startup = util::file_lock(&path.with_extension("db.startup.lock"), false).unwrap();
    let (entered, ready) = mpsc::channel();
    let (finished, result) = mpsc::channel();
    let worker_path = path.clone();
    let worker = std::thread::spawn(move || {
        entered.send(()).unwrap();
        finished
            .send(
                StateStore::open(&worker_path)
                    .map(|store| store.settings().unwrap().native_browser_discovery),
            )
            .unwrap();
    });
    ready.recv_timeout(Duration::from_secs(2)).unwrap();
    let premature = result.recv_timeout(Duration::from_millis(200));
    let opened_database = path.with_extension("db").exists();
    startup.unlock().unwrap();
    drop(startup);
    let completed = match premature {
        Ok(result) => result,
        Err(_) => result.recv_timeout(Duration::from_secs(5)).unwrap(),
    };
    worker.join().unwrap();
    assert!(
        !opened_database,
        "a second opener must wait before SQLite bootstrap"
    );
    assert_eq!(completed.unwrap(), !cfg!(target_os = "macos"));
    // The startup guard must not survive for the lifetime of the returned store.
    let store = StateStore::open(&path).unwrap();
    let next = StateStore::open(&path).unwrap();
    assert_eq!(
        store.settings().unwrap().native_browser_discovery,
        next.settings().unwrap().native_browser_discovery
    );
}

use super::*;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("hycli-codex-{}", util::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn executable(&self, relative: &str) -> PathBuf {
        let path = self.0.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"synthetic, never executed").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn stale_queued_login_cannot_replace_or_cancel_newer_ownership() {
    let fixture = Fixture::new();
    let adapter = Codex::with_binary(fixture.0.join("workspace"), fixture.0.join("missing"));
    let gate = adapter.login_gate.lock().await;
    // An older request captured revision 1, but revision 2 reached admission
    // first. Reject the old request before any ownership or protocol mutation.
    *adapter.login.lock().unwrap() = Some(LoginAttempt {
        id: "newer-owned-login".to_owned(),
        revision: 2,
        status: LoginStatus::Pending,
        activation_pending: true,
    });
    let current_revision = 2;
    let older_revision = 1;
    let mut queued = Box::pin(adapter.login_with_intent(older_revision, || {
        if current_revision != older_revision {
            return Err(AppError::api("cancelled", 409));
        }
        Ok(())
    }));
    std::future::poll_fn(|cx| {
        assert!(std::future::Future::poll(queued.as_mut(), cx).is_pending());
        std::task::Poll::Ready(())
    }).await;
    drop(gate);
    assert_eq!(queued.await.unwrap_err().public_code, "cancelled");
    assert_eq!(adapter.login_status("newer-owned-login").unwrap(),
        (LoginStatus::Pending, 2, true));
}

#[test]
fn path_precedes_documented_macos_fallback_and_returns_absolute_path() {
    let fixture = Fixture::new();
    let first = fixture.executable("first 한 글/codex");
    fixture.executable("second/codex");
    let bundled = fixture.executable("Applications/Codex.app/Contents/Resources/codex");
    let paths = [PathBuf::from("first 한 글"), PathBuf::from("second")];
    assert_eq!(
        resolve_binary(None, Some(&fixture.0), &paths, Some(&bundled), false),
        Some(first)
    );
    assert_eq!(
        resolve_binary(None, Some(&fixture.0), &[], Some(&bundled), false),
        Some(bundled)
    );
    assert_eq!(
        resolve_binary(None, Some(&fixture.0), &[], None, false),
        None
    );
}

#[test]
fn explicit_binary_is_strict_and_relative_to_construction_directory() {
    let fixture = Fixture::new();
    let selected = fixture.executable("tools 한 글/provider");
    let fallback = fixture.executable("fallback/codex");
    for request in [selected.clone(), PathBuf::from("tools 한 글/provider")] {
        assert_eq!(
            resolve_binary(
                Some(&request),
                Some(&fixture.0),
                &[],
                Some(&fallback),
                false
            ),
            Some(selected.clone())
        );
    }
    for request in ["missing", "tools/missing"] {
        assert_eq!(
            resolve_binary(
                Some(Path::new(request)),
                Some(&fixture.0),
                &[],
                Some(&fallback),
                false,
            ),
            None
        );
    }
}

#[test]
fn windows_resolves_native_exe_without_shell_shims_or_double_extension() {
    let fixture = Fixture::new();
    fixture.executable("first/codex.cmd");
    fixture.executable("first/codex");
    let native = fixture.executable("second/codex.exe");
    let paths = [fixture.0.join("first"), fixture.0.join("second")];
    for request in [None, Some(Path::new("codex")), Some(Path::new("codex.exe"))] {
        assert_eq!(
            resolve_binary(request, Some(&fixture.0), &paths, None, true),
            Some(native.clone())
        );
    }
    assert_eq!(
        resolve_binary(
            Some(Path::new("codex.cmd")),
            Some(&fixture.0),
            &paths,
            None,
            true
        ),
        None
    );
    assert_eq!(
        resolve_binary(Some(&native), Some(&fixture.0), &[], None, true),
        Some(native)
    );
}

#[test]
fn directories_and_unix_non_executable_files_are_not_available() {
    let fixture = Fixture::new();
    std::fs::create_dir(fixture.0.join("codex")).unwrap();
    assert_eq!(
        resolve_binary(None, Some(&fixture.0), &[fixture.0.clone()], None, false),
        None
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let file = fixture.executable("tools/codex");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            resolve_binary(Some(&file), Some(&fixture.0), &[], None, false),
            None
        );
    }
}

#[test]
fn missing_is_retryable_but_first_success_stays_pinned() {
    let fixture = Fixture::new();
    let selected = BinarySelection::new(None);
    let paths = [fixture.0.clone()];
    let resolve = || resolve_binary(None, Some(&fixture.0), &paths, None, false);
    assert_eq!(selected.resolve_with(resolve), None);
    let binary = fixture.executable("codex");
    assert_eq!(selected.resolve_with(resolve), Some(binary.clone()));
    assert_eq!(
        selected.resolve_with(|| panic!("A successful selection must not be replaced")),
        Some(binary)
    );
}

#[test]
fn concurrent_discovery_selects_one_binary_for_all_connections() {
    let selected = std::sync::Arc::new(BinarySelection::new(None));
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let threads: Vec<_> = (0..8)
        .map(|index| {
            let selected = selected.clone();
            let calls = calls.clone();
            std::thread::spawn(move || {
                selected.resolve_with(|| {
                    calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Some(PathBuf::from(format!("candidate-{index}")))
                })
            })
        })
        .collect();
    let paths: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert!(paths.iter().all(|path| path == &paths[0]));
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn removed_selected_executable_is_unavailable_without_forgetting_selection() {
    let fixture = Fixture::new();
    let relative = if cfg!(windows) { "codex.exe" } else { "codex" };
    let executable = fixture.executable(relative);
    let adapter = Codex::with_binary(fixture.0.join("workspace"), executable.clone());
    assert!(adapter.available());
    std::fs::remove_file(&executable).unwrap();
    assert!(!adapter.available());
    assert_eq!(
        adapter
            .binary
            .resolve_with(|| panic!("Do not switch binaries after removal")),
        Some(executable)
    );
    fixture.executable(relative);
    assert!(adapter.available());
}

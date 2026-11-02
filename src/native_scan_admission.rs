use crate::{
    apperr::{AppError, AppResult},
    util,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeScanIntent {
    UserRequested,
    Preparation,
}

/// Serializes cooperating workers sharing one data root, not unrelated data roots.
/// Move this permit into the blocking worker: caller cancellation must not release it.
pub(crate) struct NativeScanPermit {
    _lock: fs::File,
    marker: PathBuf,
    complete: bool,
}

impl NativeScanPermit {
    pub(crate) fn acquire(root: &Path, intent: NativeScanIntent) -> AppResult<Self> {
        let lock =
            util::file_lock(&root.join("native-browser-scan.lock"), true).map_err(|error| {
                if error.public_code == "conflict" {
                    AppError::api("browser_busy", 409)
                } else {
                    error
                }
            })?;
        let marker = root.join("native-browser-scan.state");
        let interrupted = match fs::symlink_metadata(&marker) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        if interrupted && intent == NativeScanIntent::Preparation {
            util::atomic_file(&marker, b"v1\nretry_required\n")?;
            return Err(AppError::api("native_discovery_retry_required", 409));
        }
        util::atomic_file(&marker, b"v1\nin_flight\n")?;
        Ok(Self {
            _lock: lock,
            marker,
            complete: false,
        })
    }

    /// Call only after the worker finished without unresolved native access or cancellation.
    pub(crate) fn clean_complete(mut self) -> AppResult<()> {
        fs::remove_file(&self.marker)?;
        self.complete = true;
        Ok(())
    }
}

impl Drop for NativeScanPermit {
    fn drop(&mut self) {
        if !self.complete {
            // An in-flight marker also blocks preparation if this write fails or the process exits.
            let _ = util::atomic_file(&self.marker, b"v1\nretry_required\n");
        }
        // Release explicitly before close, including any transient fork-inherited descriptor.
        let _ = self._lock.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        process::{Child, Command},
        time::{Duration, Instant},
    };

    fn root() -> PathBuf {
        std::env::temp_dir().join(format!("hycli-native-admission-{}", util::id()))
    }

    fn code(result: AppResult<NativeScanPermit>) -> String {
        match result {
            Err(error) => error.public_code,
            Ok(_) => panic!("scan unexpectedly admitted"),
        }
    }

    #[test]
    fn interrupted_scan_requires_explicit_retry_and_clean_success_reopens_preparation() {
        let root = root();
        drop(NativeScanPermit::acquire(&root, NativeScanIntent::Preparation).unwrap());
        assert_eq!(
            code(NativeScanPermit::acquire(
                &root,
                NativeScanIntent::Preparation
            )),
            "native_discovery_retry_required"
        );
        let retry = NativeScanPermit::acquire(&root, NativeScanIntent::UserRequested).unwrap();
        assert_eq!(
            code(NativeScanPermit::acquire(
                &root,
                NativeScanIntent::UserRequested
            )),
            "browser_busy"
        );
        retry.clean_complete().unwrap();
        NativeScanPermit::acquire(&root, NativeScanIntent::Preparation)
            .unwrap()
            .clean_complete()
            .unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unknown_marker_contents_cannot_silently_rearm_preparation() {
        let root = root();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("native-browser-scan.state"), b"unknown version").unwrap();
        assert_eq!(
            code(NativeScanPermit::acquire(
                &root,
                NativeScanIntent::Preparation
            )),
            "native_discovery_retry_required"
        );
        assert_eq!(
            fs::read(root.join("native-browser-scan.state")).unwrap(),
            b"v1\nretry_required\n"
        );
        NativeScanPermit::acquire(&root, NativeScanIntent::UserRequested)
            .unwrap()
            .clean_complete()
            .unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    fn child(root: &Path) -> Child {
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native_scan_admission::tests::native_admission_child",
                "--nocapture",
            ])
            .env("HYCLI_NATIVE_ADMISSION_CHILD", root)
            .spawn()
            .unwrap()
    }

    fn wait_for(path: &Path) {
        let until = Instant::now() + Duration::from_secs(15);
        while !path.exists() {
            assert!(
                Instant::now() < until,
                "child never acquired its scan permit"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn native_admission_child() {
        let Some(root) = std::env::var_os("HYCLI_NATIVE_ADMISSION_CHILD") else {
            return;
        };
        let root = PathBuf::from(root);
        let permit = NativeScanPermit::acquire(&root, NativeScanIntent::UserRequested).unwrap();
        fs::write(root.join("ready"), b"ready").unwrap();
        let until = Instant::now() + Duration::from_secs(20);
        while !root.join("finish").exists() {
            assert!(
                Instant::now() < until,
                "parent did not finish synthetic scan"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        permit.clean_complete().unwrap();
    }

    #[test]
    fn separate_process_holds_admission_until_actual_worker_finishes() {
        let root = root();
        let mut child = child(&root);
        wait_for(&root.join("ready"));
        assert_eq!(
            code(NativeScanPermit::acquire(
                &root,
                NativeScanIntent::Preparation
            )),
            "browser_busy"
        );
        assert_eq!(
            code(NativeScanPermit::acquire(
                &root,
                NativeScanIntent::UserRequested
            )),
            "browser_busy"
        );
        fs::write(root.join("finish"), b"finish").unwrap();
        assert!(child.wait().unwrap().success());
        NativeScanPermit::acquire(&root, NativeScanIntent::Preparation)
            .unwrap()
            .clean_complete()
            .unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn crashed_process_releases_lock_but_keeps_retry_required() {
        let root = root();
        let mut child = child(&root);
        wait_for(&root.join("ready"));
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(
            code(NativeScanPermit::acquire(
                &root,
                NativeScanIntent::Preparation
            )),
            "native_discovery_retry_required"
        );
        NativeScanPermit::acquire(&root, NativeScanIntent::UserRequested)
            .unwrap()
            .clean_complete()
            .unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}

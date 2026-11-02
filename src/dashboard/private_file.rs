//! Owner-private dashboard handoff and session artifacts. No permission repair:
//! an existing object that cannot prove its privacy is rejected before content I/O.
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

#[cfg(windows)]
mod windows;
#[cfg(unix)]
use unix::Directory;
#[cfg(windows)]
use windows::Directory;

fn denied() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "dashboard artifact is not owner-private",
    )
}

pub(super) struct PrivateFile {
    path: PathBuf,
    directory: Directory,
    owns_leaf: bool,
}

impl PrivateFile {
    pub(super) fn create(contents: &[u8]) -> io::Result<Self> {
        let parent = std::env::temp_dir().canonicalize()?;
        let path = parent.join(format!("hycli-dashboard-{}", uuid::Uuid::new_v4()));
        let directory = Directory::create(&path)?;
        let mut artifact = Self {
            path: path.join("handoff.html"),
            directory,
            owns_leaf: false,
        };
        let mut file = artifact.directory.create_file("handoff.html")?;
        artifact.owns_leaf = true;
        file.write_all(contents)?;
        file.sync_all()?;
        Ok(artifact)
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PrivateFile {
    fn drop(&mut self) {
        if self.owns_leaf {
            self.directory.remove_file("handoff.html");
        }
        self.directory.remove();
    }
}

/// Read only a bounded private control/handoff artifact named by discovery metadata.
pub(crate) fn read_private(path: &Path) -> io::Result<Vec<u8>> {
    let temp = std::env::temp_dir().canonicalize()?;
    let parent = path.parent().ok_or_else(denied)?;
    let name = parent
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(denied)?;
    let id = name.strip_prefix("hycli-dashboard-").ok_or_else(denied)?;
    if path.file_name().and_then(|s| s.to_str()) != Some("handoff.html")
        || parent.parent() != Some(temp.as_path())
        || uuid::Uuid::parse_str(id).is_err()
    {
        return Err(denied());
    }
    let directory = Directory::open(parent)?;
    let mut bytes = Vec::new();
    directory
        .open_file("handoff.html")?
        .take(4097)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "private control artifact is too large",
        ));
    }
    Ok(bytes)
}

pub(super) struct PersistentFile {
    path: PathBuf,
    file: File,
    // Retain the verified directory handle (also prevents Windows renames).
    _directory: Directory,
}

impl PersistentFile {
    pub(super) fn open_or_create(root: &Path, name: &str, initial: &[u8]) -> io::Result<Self> {
        if name != "sessions.json" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid session artifact name",
            ));
        }
        let path = root.join("dashboard-auth-v2");
        let directory = match Directory::create(&path) {
            Ok(directory) => directory,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Directory::open(&path)?,
            Err(e) => return Err(e),
        };
        let file = match directory.open_file(name) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let staging = format!("sessions-{}.tmp", uuid::Uuid::new_v4());
                let mut staged = directory.create_file(&staging)?;
                let written = staged.write_all(initial).and_then(|()| staged.sync_all());
                drop(staged);
                if let Err(error) = written {
                    directory.remove_file(&staging);
                    return Err(error);
                }
                let published = directory.publish_file(&staging, name);
                directory.remove_file(&staging);
                match published {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e),
                }
                directory.open_file(name)?
            }
            Err(e) => return Err(e),
        };
        Ok(Self {
            path: path.join(name),
            file,
            _directory: directory,
        })
    }

    pub(super) fn read(&self) -> io::Result<Vec<u8>> {
        use std::io::{Seek, SeekFrom};
        let mut file = self.file.try_clone()?;
        file.seek(SeekFrom::Start(0))?;
        let mut contents = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut contents)?;
        if contents.len() > 1024 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "session artifact is too large",
            ));
        }
        Ok(contents)
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};

    pub(super) struct Directory {
        path: PathBuf,
        handle: File,
    }

    fn verify(file: &File, directory: bool) -> io::Result<()> {
        let meta = file.metadata()?;
        // SAFETY: geteuid has no pointer arguments or side effects.
        let uid = unsafe { libc::geteuid() };
        if meta.uid() != uid
            || meta.mode() & 0o077 != 0
            || (directory && !meta.is_dir())
            || (!directory && (!meta.is_file() || meta.nlink() != 1))
        {
            return Err(denied());
        }
        Ok(())
    }

    impl Directory {
        pub(super) fn create(path: &Path) -> io::Result<Self> {
            std::fs::DirBuilder::new().mode(0o700).create(path)?;
            Self::open(path)
        }
        pub(super) fn open(path: &Path) -> io::Result<Self> {
            let handle = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(path)?;
            verify(&handle, true)?;
            Ok(Self {
                path: path.to_owned(),
                handle,
            })
        }
        fn file(&self, name: &str, create: bool) -> io::Result<File> {
            let name = CString::new(name).map_err(|_| denied())?;
            let flags = libc::O_NOFOLLOW
                | libc::O_CLOEXEC
                | if create {
                    libc::O_CREAT | libc::O_EXCL | libc::O_RDWR
                } else {
                    libc::O_RDONLY | libc::O_NONBLOCK
                };
            // SAFETY: the owned directory fd and NUL-terminated leaf are live.
            let fd = unsafe { libc::openat(self.handle.as_raw_fd(), name.as_ptr(), flags, 0o600) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: openat returned a newly owned fd.
            let file = unsafe { File::from_raw_fd(fd) };
            verify(&file, false)?;
            Ok(file)
        }
        pub(super) fn create_file(&self, name: &str) -> io::Result<File> {
            self.file(name, true)
        }
        pub(super) fn open_file(&self, name: &str) -> io::Result<File> {
            self.file(name, false)
        }
        pub(super) fn publish_file(&self, staging: &str, name: &str) -> io::Result<()> {
            let staging = CString::new(staging).map_err(|_| denied())?;
            let name = CString::new(name).map_err(|_| denied())?;
            let fd = self.handle.as_raw_fd();
            // SAFETY: both NUL-terminated leaves are relative to the retained fd.
            #[cfg(target_os = "linux")]
            let result = unsafe {
                libc::renameat2(
                    fd,
                    staging.as_ptr(),
                    fd,
                    name.as_ptr(),
                    libc::RENAME_NOREPLACE,
                )
            };
            #[cfg(target_os = "macos")]
            let result = unsafe {
                libc::renameatx_np(fd, staging.as_ptr(), fd, name.as_ptr(), libc::RENAME_EXCL)
            };
            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "atomic private session publication is unsupported",
            ));
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            {
                if result != 0 {
                    return Err(io::Error::last_os_error());
                }
                self.handle.sync_all()
            }
        }
        pub(super) fn remove_file(&self, name: &str) {
            if let Ok(name) = CString::new(name) {
                // SAFETY: unlink only this fixed leaf relative to the retained fd.
                unsafe {
                    libc::unlinkat(self.handle.as_raw_fd(), name.as_ptr(), 0);
                }
            }
        }
        pub(super) fn remove(&self) {
            if let (Ok(actual), Ok(owned)) = (
                std::fs::symlink_metadata(&self.path),
                self.handle.metadata(),
            ) {
                if actual.dev() == owned.dev() && actual.ino() == owned.ino() {
                    let _ = std::fs::remove_dir(&self.path);
                }
            }
        }
    }
}

#[cfg(not(any(unix, windows)))]
compile_error!("dashboard private artifacts require Unix permissions or Windows ACLs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_artifact_round_trip_and_cleanup() {
        let artifact = PrivateFile::create(b"dummy handoff").unwrap();
        let path = artifact.path().to_owned();
        let parent = path.parent().unwrap().to_owned();
        assert_eq!(std::fs::read(&path).unwrap(), b"dummy handoff");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&parent).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        drop(artifact);
        assert!(!path.exists());
        assert!(!parent.exists());
    }

    #[test]
    fn control_reader_accepts_only_bounded_private_artifacts() {
        let dummy = r#"{"version":1,"dummy":"control fixture"}"#;
        let artifact = PrivateFile::create(dummy.as_bytes()).unwrap();
        assert_eq!(read_private(artifact.path()).unwrap(), dummy.as_bytes());
        let oversized = PrivateFile::create(&vec![b'x'; 4097]).unwrap();
        assert!(read_private(oversized.path()).is_err());
        assert!(read_private(Path::new("sessions.json")).is_err());
    }

    #[test]
    fn existing_session_is_preserved() {
        let root = PrivateFile::create(b"dummy fixture").unwrap();
        let parent = root.path().parent().unwrap();
        let first =
            PersistentFile::open_or_create(parent, "sessions.json", b"dummy session").unwrap();
        assert_eq!(first.read().unwrap(), b"dummy session");
        drop(first);
        let second =
            PersistentFile::open_or_create(parent, "sessions.json", b"replacement").unwrap();
        assert_eq!(second.read().unwrap(), b"dummy session");
        let path = second.path().to_owned();
        drop(second);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn concurrent_session_initialization_publishes_one_complete_record() {
        let root = PrivateFile::create(b"dummy fixture").unwrap();
        let parent = root.path().parent().unwrap().to_owned();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let parent = parent.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    let session = PersistentFile::open_or_create(
                        &parent,
                        "sessions.json",
                        b"complete dummy session",
                    )
                    .unwrap();
                    assert_eq!(session.read().unwrap(), b"complete dummy session");
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let directory = parent.join("dashboard-auth-v2");
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        std::fs::remove_file(directory.join("sessions.json")).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn permissive_existing_directory_is_rejected_without_repair() {
        use std::os::unix::fs::PermissionsExt;
        let root = PrivateFile::create(b"dummy fixture").unwrap();
        let parent = root.path().parent().unwrap();
        let path = parent.join("dashboard-auth-v2");
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(PersistentFile::open_or_create(parent, "sessions.json", b"dummy").is_err());
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert!(!path.join("sessions.json").exists());
        std::fs::remove_dir(path).unwrap();
    }
}

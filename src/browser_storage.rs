//! Exact-origin localStorage snapshots. Values never leave the credential broker.
use crate::util;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub type Storage = BTreeMap<String, String>;
pub struct Snapshot(pub PathBuf);
impl Snapshot {
    pub fn new() -> Result<Self, ()> {
        let path = crate::platform::temp_dir().join(format!("hycli-session-{}", util::id()));
        util::private_dir(&path).map_err(|_| ())?;
        util::atomic_file(&path.join(".hycli-session"), b"private broker workspace")
            .map_err(|_| ())?;
        Ok(Self(path))
    }
}
impl Drop for Snapshot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn load(profile: &Path, firefox: bool, origin: &str) -> Result<Storage, ()> {
    if firefox {
        firefox_storage(profile, origin)
    } else {
        chromium_storage(profile, origin)
    }
}

/// Missing stores are empty; unreadable stores are not evidence of sign-out.
pub(crate) fn source_metadata(path: &Path) -> Result<Option<std::fs::Metadata>, ()> {
    classify_source_metadata(std::fs::metadata(path))
}
fn classify_source_metadata(
    result: std::io::Result<std::fs::Metadata>,
) -> Result<Option<std::fs::Metadata>, ()> {
    match result {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(()),
    }
}

fn chromium_string(bytes: &[u8]) -> Option<String> {
    match bytes.split_first()? {
        (1, tail) => Some(tail.iter().map(|b| char::from(*b)).collect()),
        (0, tail) if tail.len() % 2 == 0 => String::from_utf16(
            &tail
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect::<Vec<_>>(),
        )
        .ok(),
        _ => None,
    }
}

// Emit only a fixed operation and an OS/library error code in synthetic unit tests.
// Never log paths, LevelDB error messages, keys, values, or browser credentials.
fn snapshot_failure(stage: &'static str, code: impl std::fmt::Debug) {
    #[cfg(test)]
    eprintln!("browser snapshot failure: stage={stage}, code={code:?}");
    #[cfg(not(test))]
    let _ = (stage, code);
}

type FileStamp = BTreeMap<std::ffi::OsString, (u64, std::time::SystemTime)>;
type FileDigests = BTreeMap<std::ffi::OsString, String>;

fn leveldb_entry_metadata(entry: &std::fs::DirEntry) -> Result<std::fs::Metadata, ()> {
    // DirEntry metadata is cached enumeration data on Windows. Query a read-only
    // handle instead: an open writer can have a newer size than the directory entry.
    if !entry
        .file_type()
        .map_err(|error| snapshot_failure("type", error.raw_os_error()))?
        .is_file()
    {
        return Err(snapshot_failure("bounds", "unsupported"));
    }
    let file = std::fs::File::open(entry.path())
        .map_err(|error| snapshot_failure("metadata_open", error.raw_os_error()))?;
    let meta = file
        .metadata()
        .map_err(|error| snapshot_failure("metadata", error.raw_os_error()))?;
    if !meta.is_file() {
        return Err(snapshot_failure("bounds", "unsupported"));
    }
    Ok(meta)
}

fn leveldb_stamp(source: &Path) -> Result<FileStamp, ()> {
    let mut stamp = FileStamp::new();
    for entry in std::fs::read_dir(source)
        .map_err(|error| snapshot_failure("enumerate", error.raw_os_error()))?
    {
        let entry = entry.map_err(|error| snapshot_failure("entry", error.raw_os_error()))?;
        if entry.file_name() == "LOCK" {
            continue;
        }
        let meta = leveldb_entry_metadata(&entry)?;
        if meta.len() > 128 * 1024 * 1024 || stamp.len() >= 1024 {
            return Err(snapshot_failure("bounds", "unsupported"));
        }
        stamp.insert(
            entry.file_name(),
            (
                meta.len(),
                meta.modified()
                    .map_err(|error| snapshot_failure("mtime", error.raw_os_error()))?,
            ),
        );
    }
    if stamp.values().map(|(n, _)| n).sum::<u64>() > 512 * 1024 * 1024 {
        return Err(snapshot_failure("bounds", "unsupported"));
    }
    Ok(stamp)
}

fn copy_leveldb_files(source: &Path, destination: &Path) -> Result<(FileStamp, FileDigests), ()> {
    let before = leveldb_stamp(source)?;
    let mut digests = FileDigests::new();
    for (name, (size, _)) in &before {
        let bytes = util::read_bounded(&source.join(name), 128 * 1024 * 1024)
            .map_err(|error| snapshot_failure("read", error.public_code))?;
        if bytes.len() as u64 != *size {
            return Err(snapshot_failure("consistency", "size_changed"));
        }
        digests.insert(name.clone(), util::hash(&bytes));
        util::atomic_file(&destination.join(name), &bytes)
            .map_err(|error| snapshot_failure("copy", error.public_code))?;
    }
    Ok((before, digests))
}

fn validate_leveldb_contents(
    source: &Path,
    copied: &FileStamp,
    digests: &FileDigests,
) -> Result<FileStamp, ()> {
    let before = leveldb_stamp(source)?;
    // Windows may publish a live writer's last-write timestamp only when a
    // handle closes. Timestamp churn alone does not establish changed contents.
    // Require the exact file set/types/sizes and re-read every copied byte instead.
    if before.len() != copied.len()
        || before.iter().any(|(name, (size, _))| {
            copied
                .get(name)
                .is_none_or(|(old_size, _)| old_size != size)
        })
    {
        return Err(snapshot_failure("consistency", "file_set_or_size_changed"));
    }
    for (name, (size, _)) in &before {
        let bytes = util::read_bounded(&source.join(name), 128 * 1024 * 1024)
            .map_err(|error| snapshot_failure("validate_read", error.public_code))?;
        if bytes.len() as u64 != *size || digests.get(name) != Some(&util::hash(&bytes)) {
            return Err(snapshot_failure("consistency", "content_changed"));
        }
    }
    Ok(before)
}

fn finish_leveldb_validation(source: &Path, before: &FileStamp) -> Result<(), ()> {
    if &leveldb_stamp(source)? != before {
        return Err(snapshot_failure("consistency", "unstable_validation"));
    }
    Ok(())
}

fn open_chromium_leveldb(source: &Path) -> Result<(Snapshot, rusty_leveldb::DB), ()> {
    use rusty_leveldb::{DB, Options};
    let snapshot = Snapshot::new().map_err(|_| snapshot_failure("workspace", "unavailable"))?;
    // This is bounded read-only consistency validation, not an atomic filesystem
    // snapshot against arbitrary concurrent/ABA writes. Never retry a failed check.
    let (copied, digests) = copy_leveldb_files(source, &snapshot.0)?;
    let validated = validate_leveldb_contents(source, &copied, &digests)?;
    finish_leveldb_validation(source, &validated)?;
    let db = DB::open(
        &snapshot.0,
        Options {
            create_if_missing: false,
            ..Default::default()
        },
    )
    .map_err(|error| snapshot_failure("leveldb_open", error.code))?;
    Ok((snapshot, db))
}

fn decode_session_string(bytes: &[u8]) -> Option<String> {
    chromium_string(bytes).or_else(|| {
        std::str::from_utf8(bytes)
            .ok()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    })
}

fn chromium_storage(profile: &Path, origin: &str) -> Result<Storage, ()> {
    use rusty_leveldb::LdbIterator;
    let mut values = Storage::new();
    let local_source = profile.join("Local Storage/leveldb");
    if source_metadata(&local_source)?.is_some_and(|meta| meta.is_dir()) {
        let (_snapshot, mut db) = open_chromium_leveldb(&local_source)?;
        let mut iter = db
            .new_iter()
            .map_err(|error| snapshot_failure("leveldb_iterator", error.code))?;
        let prefix = format!("_{origin}\0").into_bytes();
        iter.seek(&prefix);
        loop {
            let Some((key, value)) = iter.current() else {
                break;
            };
            if !key.starts_with(&prefix) {
                break;
            }
            if value.len() <= 1024 * 1024 && values.len() < 1000 {
                if let (Some(key), Some(value)) = (
                    chromium_string(&key[prefix.len()..]),
                    chromium_string(&value),
                ) {
                    values.insert(key, value);
                }
            }
            if !iter.advance() {
                break;
            }
        }
    }
    let session_source = profile.join("Session Storage/leveldb");
    if source_metadata(&session_source)?.is_some_and(|meta| meta.is_dir()) {
        if let Ok((_snapshot, mut db)) = open_chromium_leveldb(&session_source) {
            if let Ok(mut iter) = db.new_iter() {
                let ns_prefix = b"namespace-";
                let suffix_plain = format!("-{origin}");
                let suffix_slash = format!("-{origin}/");
                let mut map_ids = std::collections::BTreeSet::new();
                iter.seek(ns_prefix);
                loop {
                    let Some((key, value)) = iter.current() else {
                        break;
                    };
                    if !key.starts_with(ns_prefix) {
                        break;
                    }
                    if let Ok(key_str) = std::str::from_utf8(&key) {
                        // Format: namespace-<session_id>-<origin>/
                        if key_str.starts_with("namespace-")
                            && (key_str.ends_with(&suffix_plain)
                                || key_str.ends_with(&suffix_slash))
                        {
                            if let Ok(map_id) = std::str::from_utf8(&value) {
                                let trimmed = map_id.trim();
                                if !trimmed.is_empty()
                                    && trimmed.bytes().all(|b| b.is_ascii_digit())
                                {
                                    map_ids.insert(trimmed.to_owned());
                                }
                            }
                        }
                    }
                    if !iter.advance() {
                        break;
                    }
                }
                for map_id in map_ids {
                    let map_prefix = format!("map-{map_id}-").into_bytes();
                    iter.seek(&map_prefix);
                    loop {
                        let Some((key, value)) = iter.current() else {
                            break;
                        };
                        if !key.starts_with(&map_prefix) {
                            break;
                        }
                        if value.len() <= 1024 * 1024 && values.len() < 1000 {
                            if let (Some(k), Some(v)) = (
                                decode_session_string(&key[map_prefix.len()..]),
                                decode_session_string(&value),
                            ) {
                                values.entry(k).or_insert(v);
                            }
                        }
                        if !iter.advance() {
                            break;
                        }
                    }
                }
            }
        }
    }
    Ok(values)
}

fn firefox_storage(profile: &Path, origin: &str) -> Result<Storage, ()> {
    let name = origin.replace("://", "+++").replace(':', "+");
    let source = profile
        .join("storage/default")
        .join(name)
        .join("ls/data.sqlite");
    if !source_metadata(&source)?.is_some_and(|meta| meta.is_file()) {
        return Ok(Storage::new());
    }
    let snapshot = Snapshot::new()?;
    let metadata =
        |p: &Path| source_metadata(p).map(|meta| meta.map(|m| (m.len(), m.modified().ok())));
    let wal = PathBuf::from(format!("{}-wal", source.display()));
    let before = (metadata(&source)?, metadata(&wal)?);
    for (from, name) in [(&source, "data.sqlite"), (&wal, "data.sqlite-wal")] {
        if source_metadata(from)?.is_some_and(|meta| meta.is_file()) {
            util::atomic_file(
                &snapshot.0.join(name),
                &util::read_bounded(from, 64 * 1024 * 1024).map_err(|_| ())?,
            )
            .map_err(|_| ())?;
        }
    }
    if before != (metadata(&source)?, metadata(&wal)?) {
        return Err(());
    }
    let db = rusqlite::Connection::open(snapshot.0.join("data.sqlite")).map_err(|_| ())?;
    let stored_origin: String = db
        .query_row("SELECT origin FROM database", [], |r| r.get(0))
        .map_err(|_| ())?;
    if stored_origin != origin {
        return Err(());
    }
    let mut query = db.prepare("SELECT key, utf16_length, conversion_type, compression_type, value FROM data LIMIT 1000").map_err(|_| ())?;
    let rows = query
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, usize>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Vec<u8>>(4)?,
            ))
        })
        .map_err(|_| ())?;
    let mut values = Storage::new();
    for row in rows {
        let (key, length, encoding, compression, bytes) = row.map_err(|_| ())?;
        if length > 512 * 1024 || bytes.len() > 1024 * 1024 {
            continue;
        }
        let bytes = match compression {
            0 => bytes,
            1 if snap::raw::decompress_len(&bytes).is_ok_and(|n| n <= 1024 * 1024) => {
                snap::raw::Decoder::new()
                    .decompress_vec(&bytes)
                    .map_err(|_| ())?
            }
            _ => continue,
        };
        let value = match encoding {
            0 if bytes.len() % 2 == 0 => String::from_utf16(
                &bytes
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect::<Vec<_>>(),
            )
            .ok(),
            1 => String::from_utf8(bytes).ok(),
            _ => None,
        };
        if let Some(value) = value.filter(|v| v.encode_utf16().count() == length) {
            values.insert(key, value);
        }
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn owned_snapshot_fixture() -> (Snapshot, PathBuf, PathBuf) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/tests")
            .join(util::id());
        let source = root.join("source");
        let destination = root.join("snapshot");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::write(source.join("000001.log"), b"synthetic-a").unwrap();
        (Snapshot(root), source, destination)
    }

    #[test]
    fn source_metadata_errors_are_not_empty_stores() {
        use std::io::{Error, ErrorKind};
        assert!(
            classify_source_metadata(Err(Error::from(ErrorKind::NotFound)))
                .unwrap()
                .is_none()
        );
        for kind in [
            ErrorKind::PermissionDenied,
            ErrorKind::Interrupted,
            ErrorKind::Other,
        ] {
            assert!(classify_source_metadata(Err(Error::from(kind))).is_err());
        }
        let (_root, source, _) = owned_snapshot_fixture();
        assert!(source_metadata(&source).unwrap().unwrap().is_dir());
        let file = source.join("000001.log");
        assert!(source_metadata(&file).unwrap().unwrap().is_file());
        assert!(source_metadata(&source.join("missing")).unwrap().is_none());
    }

    #[test]
    fn snapshot_queries_current_handle_after_directory_entry_was_cached() {
        use std::io::Write;
        let (_root, source, destination) = owned_snapshot_fixture();
        let path = source.join("000001.log");
        let entry = std::fs::read_dir(&source).unwrap().next().unwrap().unwrap();
        let mut writer = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        writer.write_all(b"-grown-after-enumeration").unwrap();
        writer.flush().unwrap();
        // Keep both writer and old directory entry alive. Windows DirEntry caches
        // the old length, while the read-only handle must see the actual bytes.
        let expected = std::fs::read(&path).unwrap();
        #[cfg(windows)]
        assert_ne!(
            entry.metadata().unwrap().len(),
            expected.len() as u64,
            "Owned fixture must reproduce cached Windows enumeration length"
        );
        assert_eq!(
            leveldb_entry_metadata(&entry).unwrap().len(),
            expected.len() as u64
        );
        let (stamp, digests) = copy_leveldb_files(&source, &destination).unwrap();
        let validated = validate_leveldb_contents(&source, &stamp, &digests).unwrap();
        finish_leveldb_validation(&source, &validated).unwrap();
        assert_eq!(
            std::fs::read(destination.join("000001.log")).unwrap(),
            expected
        );
        writer.write_all(b"-writer-still-owned").unwrap();
        writer.flush().unwrap();
        assert!(validate_leveldb_contents(&source, &stamp, &digests).is_err());
    }

    #[test]
    fn snapshot_accepts_metadata_only_change_after_copy() {
        let (_root, source, destination) = owned_snapshot_fixture();
        let (stamp, digests) = copy_leveldb_files(&source, &destination).unwrap();
        let path = source.join("000001.log");
        let changed = stamp.values().next().unwrap().1 + std::time::Duration::from_secs(10);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(changed))
            .unwrap();
        assert_ne!(leveldb_stamp(&source).unwrap(), stamp);
        let validated = validate_leveldb_contents(&source, &stamp, &digests).unwrap();
        finish_leveldb_validation(&source, &validated).unwrap();
        assert_eq!(
            std::fs::read(path).unwrap(),
            std::fs::read(destination.join("000001.log")).unwrap()
        );
    }

    #[test]
    fn snapshot_rejects_changed_content_even_with_same_size_and_timestamp() {
        let (_root, source, destination) = owned_snapshot_fixture();
        let (stamp, digests) = copy_leveldb_files(&source, &destination).unwrap();
        let path = source.join("000001.log");
        std::fs::write(&path, b"synthetic-b").unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(stamp.values().next().unwrap().1))
            .unwrap();
        assert_eq!(leveldb_stamp(&source).unwrap(), stamp);
        assert!(validate_leveldb_contents(&source, &stamp, &digests).is_err());
    }

    #[test]
    fn snapshot_rejects_added_removed_or_retyped_files() {
        for change in ["added", "removed", "retyped"] {
            let (_root, source, destination) = owned_snapshot_fixture();
            let (stamp, digests) = copy_leveldb_files(&source, &destination).unwrap();
            match change {
                "added" => std::fs::write(source.join("000002.log"), b"new").unwrap(),
                "removed" => std::fs::remove_file(source.join("000001.log")).unwrap(),
                _ => {
                    std::fs::remove_file(source.join("000001.log")).unwrap();
                    std::fs::create_dir(source.join("000001.log")).unwrap();
                }
            }
            assert!(validate_leveldb_contents(&source, &stamp, &digests).is_err());
        }
    }

    #[test]
    fn snapshot_rejects_source_changing_during_validation() {
        let (_root, source, destination) = owned_snapshot_fixture();
        let (stamp, digests) = copy_leveldb_files(&source, &destination).unwrap();
        let validated = validate_leveldb_contents(&source, &stamp, &digests).unwrap();
        std::fs::write(source.join("000002.log"), b"new").unwrap();
        assert!(finish_leveldb_validation(&source, &validated).is_err());
    }

    #[test]
    fn decodes_chromium_latin1_and_utf16_without_treating_latin1_as_utf8() {
        assert_eq!(chromium_string(&[1, 0xe9]), Some("é".into()));
        assert_eq!(chromium_string(&[0, 0x00, 0xac]), Some("가".into()));
        assert_eq!(chromium_string(&[0, 0x00]), None);
        assert_eq!(chromium_string(&[1]), Some(String::new()));
    }
    #[test]
    fn chromium_snapshot_excludes_other_origins_and_partitioned_storage() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".cache/tests")
            .join(util::id());
        let path = root.join("Local Storage/leveldb");
        std::fs::create_dir_all(&path).unwrap();
        let mut db = rusty_leveldb::DB::open(&path, Default::default()).unwrap();
        for (key, value) in [
            (
                b"_https://example.com\0\x01session".as_slice(),
                b"\x01synthetic-owned".as_slice(),
            ),
            (
                b"_https://example.com/^0https://other.example\0\x01session".as_slice(),
                b"\x01partitioned".as_slice(),
            ),
            (
                b"_https://example.com.evil\0\x01session".as_slice(),
                b"\x01other".as_slice(),
            ),
        ] {
            db.put(key, value).unwrap();
        }
        db.flush().unwrap();
        let storage = chromium_storage(&root, "https://example.com").unwrap();
        assert_eq!(
            storage,
            Storage::from([("session".into(), "synthetic-owned".into())])
        );
        // The live source remains open/locked throughout the snapshot read.
        assert_eq!(
            db.get(b"_https://example.com\0\x01session")
                .unwrap()
                .as_ref(),
            b"\x01synthetic-owned"
        );
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}

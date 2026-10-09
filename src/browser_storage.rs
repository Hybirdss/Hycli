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
        let path = std::env::temp_dir().join(format!("hycli-session-{}", util::id()));
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

fn chromium_storage(profile: &Path, origin: &str) -> Result<Storage, ()> {
    use rusty_leveldb::{DB, LdbIterator, Options};
    let source = profile.join("Local Storage/leveldb");
    if !source.is_dir() {
        return Ok(Storage::new());
    }
    let snapshot = Snapshot::new()?;
    let stamp = |path: &Path| -> Result<BTreeMap<String, (u64, std::time::SystemTime)>, ()> {
        std::fs::read_dir(path)
            .map_err(|_| ())?
            .filter_map(Result::ok)
            .filter(|e| e.file_name() != "LOCK")
            .map(|e| {
                let m = e.metadata().map_err(|_| ())?;
                if !m.is_file() || m.len() > 128 * 1024 * 1024 {
                    return Err(());
                }
                Ok((
                    e.file_name().to_string_lossy().into_owned(),
                    (m.len(), m.modified().map_err(|_| ())?),
                ))
            })
            .collect()
    };
    let before = stamp(&source)?;
    if before.len() > 1024 || before.values().map(|(n, _)| n).sum::<u64>() > 512 * 1024 * 1024 {
        return Err(());
    }
    for name in before.keys() {
        let bytes = util::read_bounded(&source.join(name), 128 * 1024 * 1024).map_err(|_| ())?;
        util::atomic_file(&snapshot.0.join(name), &bytes).map_err(|_| ())?;
    }
    if before != stamp(&source)? {
        return Err(());
    }
    let mut db = DB::open(
        &snapshot.0,
        Options {
            create_if_missing: false,
            ..Default::default()
        },
    )
    .map_err(|_| ())?;
    let mut iter = db.new_iter().map_err(|_| ())?;
    let prefix = format!("_{origin}\0").into_bytes();
    iter.seek(&prefix);
    let mut values = Storage::new();
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
    Ok(values)
}

fn firefox_storage(profile: &Path, origin: &str) -> Result<Storage, ()> {
    let name = origin.replace("://", "+++").replace(':', "+");
    let source = profile
        .join("storage/default")
        .join(name)
        .join("ls/data.sqlite");
    if !source.is_file() {
        return Ok(Storage::new());
    }
    let snapshot = Snapshot::new()?;
    let metadata = |p: &Path| {
        std::fs::metadata(p)
            .ok()
            .map(|m| (m.len(), m.modified().ok()))
    };
    let wal = PathBuf::from(format!("{}-wal", source.display()));
    let before = (metadata(&source), metadata(&wal));
    for (from, name) in [(&source, "data.sqlite"), (&wal, "data.sqlite-wal")] {
        if from.is_file() {
            util::atomic_file(
                &snapshot.0.join(name),
                &util::read_bounded(from, 64 * 1024 * 1024).map_err(|_| ())?,
            )
            .map_err(|_| ())?;
        }
    }
    if before != (metadata(&source), metadata(&wal)) {
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

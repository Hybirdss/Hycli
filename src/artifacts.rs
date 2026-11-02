use crate::apperr::{AppError, AppResult};
use crate::util;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

pub const TRANSFER_CHUNK_BYTES: usize = 64 * 1024;
pub const INSPECTION_PREFIX_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub site: String,
    pub action: String,
    pub source_origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompletedArtifact {
    pub kind: String,
    pub version: u32,
    pub artifact_id: String,
    pub status: String,
    pub local_path: String,
    pub location_scope: String,
    pub filename: String,
    pub media_type: String,
    pub byte_count: u64,
    pub sha256: String,
    pub created_at: String,
    pub provenance: Provenance,
    pub content_trust: String,
}

#[derive(Clone)]
pub struct ArtifactStore {
    root: PathBuf,
}

// Every issued disk operation keeps the owner alive. Cancellation may finish one
// bounded write, but cannot publish it or remove its directory while it is open.
struct Stage {
    directory: PathBuf,
    data: Mutex<StageData>,
    retained: AtomicBool,
}
struct StageData {
    file: Option<File>,
    hash: Sha256,
    byte_count: u64,
    prefix: Vec<u8>,
}
pub struct WritableArtifact {
    stage: Arc<Stage>,
}
pub struct SealedArtifact {
    stage: Arc<Stage>,
    byte_count: u64,
    sha256: String,
    prefix: Vec<u8>,
}
pub struct PublishedArtifact {
    stage: Arc<Stage>,
    manifest: CompletedArtifact,
}
pub struct OpenedArtifact {
    pub manifest: CompletedArtifact,
    file: tokio::fs::File,
}

fn digest_hex(hash: Sha256) -> String {
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn invalid() -> AppError {
    AppError::api("data_invalid", 400)
}
fn payload_filename(filename: &str, media_type: &str) -> String {
    let known = match media_type {
        "application/pdf" => Some("pdf"),
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/svg+xml" => Some("svg"),
        "application/json" => Some("json"),
        "text/csv" => Some("csv"),
        "text/plain" => Some("txt"),
        "text/html" => Some("html"),
        "application/xml" | "text/xml" => Some("xml"),
        "application/zip" => Some("zip"),
        "application/gzip" => Some("gz"),
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => Some("xlsx"),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => Some("docx"),
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => Some("pptx"),
        _ => None,
    };
    let suffix = filename
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    let suffix = known.unwrap_or_else(|| {
        if [
            "pdf", "png", "jpg", "jpeg", "gif", "webp", "svg", "avif", "csv", "txt", "json", "xml",
            "zip", "gz", "tar", "xlsx", "xls", "docx", "doc", "pptx", "ppt", "ods", "odt", "odp",
            "html", "htm",
        ]
        .contains(&suffix.as_str())
        {
            suffix.as_str()
        } else {
            "bin"
        }
    });
    format!("payload.{suffix}")
}
fn lookup_error(error: std::io::Error) -> AppError {
    if error.kind() == std::io::ErrorKind::NotFound {
        AppError::api("not_found", 404)
    } else {
        error.into()
    }
}
fn checked_directory(path: &Path) -> AppResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(lookup_error)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid());
    }
    Ok(())
}
fn private_new_file(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
fn create_private_directory(path: &Path) -> AppResult<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    Ok(())
}
struct ArtifactDirectory {
    handle: File,
    #[cfg(not(unix))]
    path: PathBuf,
}
fn check_opened(file: &File, directory: bool) -> AppResult<()> {
    let metadata = file.metadata()?;
    if metadata.file_type().is_symlink()
        || if directory {
            !metadata.is_dir()
        } else {
            !metadata.is_file()
        }
    {
        return Err(invalid());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(invalid());
        }
    }
    Ok(())
}
impl ArtifactDirectory {
    fn open(path: &Path) -> AppResult<Self> {
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            // Keep the directory from being renamed while opening its children.
            options.custom_flags(0x02000000 | 0x00200000).share_mode(3);
        }
        let handle = options.open(path).map_err(lookup_error)?;
        check_opened(&handle, true)?;
        Ok(Self {
            handle,
            #[cfg(not(unix))]
            path: path.to_owned(),
        })
    }
    fn open_child(&self, name: &str, directory: bool) -> AppResult<File> {
        #[cfg(unix)]
        let file = {
            use std::os::fd::{AsRawFd, FromRawFd};
            let name = std::ffi::CString::new(name).map_err(|_| invalid())?;
            let flags = libc::O_RDONLY
                | libc::O_CLOEXEC
                | libc::O_NOFOLLOW
                | libc::O_NONBLOCK
                | if directory { libc::O_DIRECTORY } else { 0 };
            let fd = unsafe { libc::openat(self.handle.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 {
                return Err(lookup_error(std::io::Error::last_os_error()));
            }
            unsafe { File::from_raw_fd(fd) }
        };
        #[cfg(not(unix))]
        let file = {
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options
                    .custom_flags(0x00200000 | if directory { 0x02000000 } else { 0 })
                    .share_mode(3);
            }
            options.open(self.path.join(name)).map_err(lookup_error)?
        };
        check_opened(&file, directory)?;
        Ok(file)
    }
    fn child(&self, name: &str) -> AppResult<Self> {
        Ok(Self {
            handle: self.open_child(name, true)?,
            #[cfg(not(unix))]
            path: self.path.join(name),
        })
    }
}
fn read_manifest(directory: &ArtifactDirectory) -> AppResult<Vec<u8>> {
    let mut bytes = Vec::new();
    directory
        .open_child("manifest.json", false)?
        .take(16385)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 16384 {
        return Err(invalid());
    }
    Ok(bytes)
}
impl ArtifactStore {
    pub fn open(runtime_root: &Path) -> AppResult<Self> {
        checked_directory(runtime_root)?;
        let root = runtime_root.join("artifacts");
        match create_private_directory(&root) {
            Ok(()) => {}
            Err(error) if root.try_exists().unwrap_or(false) => {
                checked_directory(&root).map_err(|_| error)?;
            }
            Err(error) => return Err(error),
        }
        checked_directory(&root)?;
        #[cfg(unix)]
        File::open(runtime_root)?.sync_all()?;
        Ok(Self {
            root: fs::canonicalize(root)?,
        })
    }
    pub fn begin(&self) -> AppResult<WritableArtifact> {
        checked_directory(&self.root)?;
        let directory = self.root.join(util::id());
        // Reject unrepresentable paths before allocating a stage.
        directory.to_str().ok_or_else(invalid)?;
        create_private_directory(&directory)?;
        let stage = Arc::new(Stage {
            directory,
            data: Mutex::new(StageData {
                file: None,
                hash: Sha256::new(),
                byte_count: 0,
                prefix: Vec::new(),
            }),
            retained: AtomicBool::new(false),
        });
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(stage.directory.join("payload.pending"))?;
        stage.data.lock().map_err(|_| invalid())?.file = Some(file);
        Ok(WritableArtifact { stage })
    }
    pub fn open_completed(&self, id: &str) -> AppResult<OpenedArtifact> {
        if id.len() != 32
            || !id
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(invalid());
        }
        let root_handle = ArtifactDirectory::open(&self.root)?;
        let directory_handle = root_handle.child(id)?;
        let directory = self.root.join(id);
        let manifest: CompletedArtifact =
            serde_json::from_slice(&read_manifest(&directory_handle)?)?;
        let payload = directory.join(payload_filename(&manifest.filename, &manifest.media_type));
        if manifest.kind != "file"
            || manifest.version != 1
            || manifest.status != "complete"
            || manifest.artifact_id != id
            || manifest.location_scope != "hycli_host"
            || manifest.content_trust != "untrusted"
            || manifest.local_path != payload.to_str().ok_or_else(invalid)?
            || manifest.filename != safe_filename(&manifest.filename)
            || manifest.sha256.len() != 64
            || !manifest
                .sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(invalid());
        }
        let file = directory_handle.open_child(
            &payload_filename(&manifest.filename, &manifest.media_type),
            false,
        )?;
        if file.metadata()?.len() != manifest.byte_count {
            return Err(invalid());
        }
        Ok(OpenedArtifact {
            manifest,
            file: tokio::fs::File::from_std(file),
        })
    }
}
impl WritableArtifact {
    pub async fn write(&mut self, bytes: &[u8]) -> AppResult<()> {
        for part in bytes.chunks(TRANSFER_CHUNK_BYTES) {
            let bytes = part.to_vec();
            let stage = self.stage.clone();
            tokio::task::spawn_blocking(move || -> AppResult<()> {
                let mut data = stage.data.lock().map_err(|_| invalid())?;
                let count = data
                    .byte_count
                    .checked_add(bytes.len() as u64)
                    .ok_or_else(invalid)?;
                data.file.as_mut().ok_or_else(invalid)?.write_all(&bytes)?;
                data.hash.update(&bytes);
                data.byte_count = count;
                let take = bytes
                    .len()
                    .min(INSPECTION_PREFIX_BYTES.saturating_sub(data.prefix.len()));
                data.prefix.extend_from_slice(&bytes[..take]);
                Ok(())
            })
            .await
            .map_err(|_| AppError::api("internal", 500))??;
        }
        Ok(())
    }
    pub async fn seal(self) -> AppResult<SealedArtifact> {
        let stage = self.stage.clone();
        let (byte_count, sha256, prefix) = tokio::task::spawn_blocking(move || -> AppResult<_> {
            let mut data = stage.data.lock().map_err(|_| invalid())?;
            let mut file = data.file.take().ok_or_else(invalid)?;
            file.flush()?;
            file.sync_all()?;
            drop(file);
            Ok((
                data.byte_count,
                digest_hex(data.hash.clone()),
                data.prefix.clone(),
            ))
        })
        .await
        .map_err(|_| AppError::api("internal", 500))??;
        Ok(SealedArtifact {
            stage: self.stage,
            byte_count,
            sha256,
            prefix,
        })
    }
}
impl SealedArtifact {
    pub fn byte_count(&self) -> u64 {
        self.byte_count
    }
    pub fn prefix(&self) -> &[u8] {
        &self.prefix
    }
    pub fn publish(
        self,
        filename: &str,
        media_type: &str,
        provenance: Provenance,
    ) -> AppResult<PublishedArtifact> {
        let filename = safe_filename(filename);
        let payload = self
            .stage
            .directory
            .join(payload_filename(&filename, media_type));
        // Hard links never replace an existing payload/manifest.
        fs::hard_link(self.stage.directory.join("payload.pending"), &payload)?;
        fs::remove_file(self.stage.directory.join("payload.pending"))?;
        let manifest = CompletedArtifact {
            kind: "file".into(),
            version: 1,
            artifact_id: self
                .stage
                .directory
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(invalid)?
                .into(),
            status: "complete".into(),
            local_path: payload.to_str().ok_or_else(invalid)?.into(),
            location_scope: "hycli_host".into(),
            filename,
            media_type: media_type.into(),
            byte_count: self.byte_count,
            sha256: self.sha256,
            created_at: util::now(),
            provenance,
            content_trust: "untrusted".into(),
        };
        private_new_file(
            &self.stage.directory.join("manifest.pending"),
            &serde_json::to_vec(&manifest)?,
        )?;
        fs::hard_link(
            self.stage.directory.join("manifest.pending"),
            self.stage.directory.join("manifest.json"),
        )?;
        fs::remove_file(self.stage.directory.join("manifest.pending"))?;
        #[cfg(unix)]
        {
            File::open(&self.stage.directory)?.sync_all()?;
            File::open(self.stage.directory.parent().ok_or_else(invalid)?)?.sync_all()?;
        }
        Ok(PublishedArtifact {
            stage: self.stage,
            manifest,
        })
    }
}
impl PublishedArtifact {
    pub fn manifest(&self) -> &CompletedArtifact {
        &self.manifest
    }
    pub fn persisted(self) -> CompletedArtifact {
        self.stage.retained.store(true, Ordering::Release);
        self.manifest.clone()
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        if !self.retained.load(Ordering::Acquire) {
            // This is the last Arc, including any one issued blocking write.
            match self.data.get_mut() {
                Ok(data) => {
                    drop(data.file.take());
                }
                Err(poisoned) => {
                    drop(poisoned.into_inner().file.take());
                }
            }
            // A failed cleanup leaves an unlisted orphan, never a completed job.
            // Do not sweep other processes' stages or retained artifacts by age.
            if let Err(error) = fs::remove_dir_all(&self.directory) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("hycli: incomplete file cleanup failed ({:?})", error.kind());
                }
            }
        }
    }
}
impl OpenedArtifact {
    /// Only successful EOF certifies emitted bytes. Provisional bytes cannot be
    /// retracted; callers MUST NOT add Content-Length or a known upper size hint.
    pub fn into_stream(self) -> impl futures_util::Stream<Item = std::io::Result<Vec<u8>>> + Send {
        let state = (self.file, self.manifest, Sha256::new(), 0u64);
        futures_util::stream::try_unfold(
            state,
            |(mut file, manifest, mut hash, count)| async move {
                use tokio::io::AsyncReadExt;
                let mut bytes = vec![0; TRANSFER_CHUNK_BYTES];
                let n = file.read(&mut bytes).await?;
                if n == 0 {
                    if count != manifest.byte_count || digest_hex(hash) != manifest.sha256 {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "artifact integrity mismatch",
                        ));
                    }
                    return Ok(None);
                }
                let count = count.checked_add(n as u64).ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "artifact length overflow")
                })?;
                if count > manifest.byte_count {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "artifact length mismatch",
                    ));
                }
                bytes.truncate(n);
                hash.update(&bytes);
                Ok(Some((bytes, (file, manifest, hash, count))))
            },
        )
    }
}

pub fn safe_filename(value: &str) -> String {
    let name: String = value
        .chars()
        .filter(|c| {
            !c.is_control()
                && !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
                && !('\u{202a}'..='\u{202e}').contains(c)
                && !('\u{2066}'..='\u{2069}').contains(c)
        })
        .take(120)
        .collect();
    let name = name.trim_matches(|c: char| c == '.' || c.is_whitespace());
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if name.is_empty()
        || [
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ]
        .contains(&stem.as_str())
    {
        "download.bin".into()
    } else {
        name.into()
    }
}
pub fn response_filename(content_disposition: &str) -> String {
    if content_disposition.len() > 4096 {
        return "download.bin".into();
    }
    let mut fields = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in content_disposition.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            quoted = !quoted;
        }
        if ch == ';' && !quoted {
            fields.push(&content_disposition[start..index]);
            start = index + 1;
        }
    }
    fields.push(&content_disposition[start..]);
    let mut fallback = None;
    for field in fields.into_iter().skip(1) {
        let Some((key, value)) = field.trim().split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("filename*") {
            let mut parts = value.trim().splitn(3, '\'');
            if !parts.next().unwrap_or("").eq_ignore_ascii_case("utf-8") {
                continue;
            }
            let _language = parts.next();
            if let Some(encoded) = parts.next().and_then(decode_filename) {
                return safe_filename(&encoded);
            }
        } else if key.trim().eq_ignore_ascii_case("filename") && fallback.is_none() {
            fallback = Some(safe_filename(value.trim().trim_matches('"')));
        }
    }
    fallback.unwrap_or_else(|| "download.bin".into())
}
fn decode_filename(value: &str) -> Option<String> {
    let mut decoded = Vec::new();
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = (*bytes.get(index + 1)? as char).to_digit(16)?;
            let low = (*bytes.get(index + 2)? as char).to_digit(16)?;
            decoded.push((high * 16 + low) as u8);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{Stream, StreamExt, pin_mut};
    fn root() -> PathBuf {
        let path = std::env::temp_dir().join(format!("hycli-artifact-{}", util::id()));
        util::private_dir(&path).unwrap();
        path
    }
    fn provenance() -> Provenance {
        Provenance {
            site: "fixture".into(),
            action: "download".into(),
            source_origin: "https://example.test".into(),
        }
    }
    async fn publish(store: &ArtifactStore, bytes: &[u8]) -> PublishedArtifact {
        let mut writable = store.begin().unwrap();
        writable.write(bytes).await.unwrap();
        writable
            .seal()
            .await
            .unwrap()
            .publish("file.bin", "application/octet-stream", provenance())
            .unwrap()
    }
    #[tokio::test]
    async fn stage_and_publication_keep_cleanup_until_persistence() {
        let root = root();
        let store = ArtifactStore::open(&root).unwrap();
        let mut writable = store.begin().unwrap();
        let directory = writable.stage.directory.clone();
        writable.write(b"one").await.unwrap();
        assert!(
            store
                .open_completed(directory.file_name().unwrap().to_str().unwrap())
                .is_err()
        );
        drop(writable);
        assert!(!directory.exists());
        let published = publish(&store, b"one").await;
        let directory = published.stage.directory.clone();
        drop(published);
        assert!(!directory.exists());
        let retained = publish(&store, b"two").await.persisted();
        assert_eq!(fs::read(&retained.local_path).unwrap(), b"two");
        let mut other = store.begin().unwrap();
        other.write(b"partial").await.unwrap();
        drop(other);
        assert!(Path::new(&retained.local_path).exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn large_producer_and_consumer_are_chunk_bounded_above_old_limits() {
        let root = root();
        let store = ArtifactStore::open(&root).unwrap();
        let mut writable = store.begin().unwrap();
        let chunk = [0x73; TRANSFER_CHUNK_BYTES];
        let chunks = 1601; // More than both former 8 MiB and 100 MiB ceilings.
        let mut expected = Sha256::new();
        for _ in 0..chunks {
            writable.write(&chunk).await.unwrap();
            expected.update(chunk);
        }
        let sealed = writable.seal().await.unwrap();
        assert_eq!(sealed.prefix().len(), INSPECTION_PREFIX_BYTES);
        let file = sealed
            .publish("large.bin", "application/octet-stream", provenance())
            .unwrap()
            .persisted();
        assert_eq!(file.byte_count, (chunks * TRANSFER_CHUNK_BYTES) as u64);
        assert_eq!(file.sha256, digest_hex(expected));
        let stream = store
            .open_completed(&file.artifact_id)
            .unwrap()
            .into_stream();
        assert_eq!(stream.size_hint().1, None);
        pin_mut!(stream);
        let mut count = 0u64;
        let mut actual = Sha256::new();
        let mut largest = 0;
        while let Some(part) = stream.next().await {
            let part = part.unwrap();
            largest = largest.max(part.len());
            count += part.len() as u64;
            actual.update(part);
        }
        assert!(largest <= TRANSFER_CHUNK_BYTES);
        assert_eq!(count, file.byte_count);
        assert_eq!(digest_hex(actual), file.sha256);
        fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn corrupted_same_length_payload_errors_at_eof_not_success() {
        let root = root();
        let store = ArtifactStore::open(&root).unwrap();
        let file = publish(&store, b"one").await.persisted();
        fs::write(&file.local_path, b"two").unwrap();
        let stream = store
            .open_completed(&file.artifact_id)
            .unwrap()
            .into_stream();
        pin_mut!(stream);
        assert_eq!(stream.next().await.unwrap().unwrap(), b"two");
        assert!(stream.next().await.unwrap().is_err());
        assert!(stream.next().await.is_none());
        fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn file_write_failure_leaves_no_completed_or_partial_artifact() {
        let root = root();
        let store = ArtifactStore::open(&root).unwrap();
        let mut writable = store.begin().unwrap();
        let directory = writable.stage.directory.clone();
        // Replace only this stage's handle with a read-only handle: deterministic
        // write failure without relying on the host disk being full.
        writable.stage.data.lock().unwrap().file =
            Some(File::open(directory.join("payload.pending")).unwrap());
        assert!(writable.write(b"cannot write").await.is_err());
        drop(writable);
        assert!(!directory.exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn dropped_write_owner_waits_for_issued_io_before_cleanup() {
        let root = root();
        let store = ArtifactStore::open(&root).unwrap();
        let writable = store.begin().unwrap();
        let stage = writable.stage.clone();
        let directory = stage.directory.clone();
        drop(writable);
        assert!(directory.exists());
        // A bounded I/O operation retains exactly this owner on cancellation.
        drop(stage);
        assert!(!directory.exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn refuses_payload_symlinks_and_artifact_root_symlinks() {
        let root = root();
        let store = ArtifactStore::open(&root).unwrap();
        let file = publish(&store, b"one").await.persisted();
        let target = root.join("other");
        fs::write(&target, b"one").unwrap();
        fs::remove_file(&file.local_path).unwrap();
        std::os::unix::fs::symlink(&target, &file.local_path).unwrap();
        assert!(store.open_completed(&file.artifact_id).is_err());
        fs::remove_dir_all(root).unwrap();
        let root = self::root();
        let other = root.join("other");
        fs::create_dir(&other).unwrap();
        std::os::unix::fs::symlink(other, root.join("artifacts")).unwrap();
        assert!(ArtifactStore::open(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn filenames_remain_safe_and_international() {
        assert_eq!(safe_filename("CON.txt"), "download.bin");
        assert_eq!(
            response_filename(
                "attachment; filename=fallback.pdf; filename*=UTF-8''%EB%B3%B4%EA%B3%A0%EC%84%9C.pdf"
            ),
            "보고서.pdf"
        );
        assert_eq!(
            response_filename("attachment; filename=\"report; final.pdf\""),
            "report; final.pdf"
        );
    }
}

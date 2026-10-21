//! Browser inventory is metadata only. Site-scoped session import stays inside the local broker.
pub use crate::environment::BrowserProfile;
#[cfg(test)]
use crate::environment::firefox_profiles;
use crate::{
    apperr::{AppError, AppResult},
    runtime::Runtime,
    util,
};
use rookie_cookies::CancellationHandle;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub fn discover() -> Vec<BrowserProfile> {
    crate::environment::inspect().profiles
}

#[derive(Clone, Serialize)]
pub struct BrowserSearch {
    pub profiles_checked: usize,
    pub profiles_unavailable: usize,
    pub accounts: Vec<crate::accounts::AccountView>,
    pub selected_account: String,
    pub needs_choice: bool,
    pub login_url: String,
}
struct ImportedSession {
    profile: BrowserProfile,
    cookies: Vec<crate::accounts::Cookie>,
    storage: crate::browser_storage::Storage,
    complete: bool,
}
impl ImportedSession {
    fn from_results(
        profile: BrowserProfile,
        cookies: Result<Vec<crate::accounts::Cookie>, ()>,
        storage: Result<crate::browser_storage::Storage, ()>,
    ) -> Self {
        Self {
            profile,
            complete: cookies.is_ok() && storage.is_ok(),
            cookies: cookies.unwrap_or_default(),
            storage: storage.unwrap_or_default(),
        }
    }
}
struct ChromiumSnapshot(PathBuf);
impl Drop for ChromiumSnapshot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
impl ChromiumSnapshot {
    fn create(source: &Path) -> Result<Self, ()> {
        let snapshot =
            Self(crate::platform::temp_dir().join(format!("hycli-browser-{}", util::id())));
        util::private_dir(&snapshot.0).map_err(|_| ())?;
        let wal = PathBuf::from(format!("{}-wal", source.display()));
        let stamp = |path: &Path| -> Result<Option<(u64, std::time::SystemTime)>, ()> {
            match std::fs::metadata(path) {
                Ok(meta) => Ok(Some((meta.len(), meta.modified().map_err(|_| ())?))),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(_) => Err(()),
            }
        };
        // Inspect and decrypt exactly the same private snapshot. Never open the user's DB for writes.
        let before = (stamp(source)?, stamp(&wal)?);
        for (from, name) in [(source, "Cookies"), (wal.as_path(), "Cookies-wal")] {
            if stamp(from)?.is_some() {
                let bytes = util::read_bounded(from, 256 * 1024 * 1024).map_err(|_| ())?;
                util::atomic_file(&snapshot.0.join(name), &bytes).map_err(|_| ())?;
            }
        }
        if before != (stamp(source)?, stamp(&wal)?) {
            return Err(());
        }
        let connection = rusqlite::Connection::open_with_flags(
            snapshot.0.join("Cookies"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(|_| ())?;
        chromium_isolation_known(&connection)?;
        drop(connection);
        Ok(snapshot)
    }
}
fn chromium_isolation_known(connection: &rusqlite::Connection) -> Result<(), ()> {
    // rookie 0.6 projects both unknown and known-empty partition keys to None.
    // Validate the source first so None can safely be restored to a known-empty key.
    let invalid: i64 = connection
        .query_row(
            "SELECT count(*) FROM cookies WHERE typeof(top_frame_site_key) != 'text' OR typeof(has_cross_site_ancestor) != 'integer' OR has_cross_site_ancestor NOT IN (0,1)",
            [],
            |row| row.get(0),
        )
        .map_err(|_| ())?;
    if invalid == 0 { Ok(()) } else { Err(()) }
}
#[derive(Debug, PartialEq, Eq)]
enum CookieLoadFailure {
    Unavailable,
    Unresolved,
}
impl From<()> for CookieLoadFailure {
    fn from(_: ()) -> Self {
        Self::Unavailable
    }
}

fn load_site_cookies(
    profile: &BrowserProfile,
    target: &url::Url,
    cancellation: &CancellationHandle,
) -> Result<Vec<crate::accounts::Cookie>, CookieLoadFailure> {
    if cancellation.is_cancelled() {
        return Err(CookieLoadFailure::Unavailable);
    }
    use rookie_cookies::{AppBoundPolicy, FromPathRequest};
    let db = if profile.browser == "Firefox" {
        profile.path.join("cookies.sqlite")
    } else if crate::browser_storage::source_metadata(&profile.path.join("Network/Cookies"))?
        .is_some_and(|meta| meta.is_file())
    {
        profile.path.join("Network/Cookies")
    } else {
        profile.path.join("Cookies")
    };
    if !crate::browser_storage::source_metadata(&db)?.is_some_and(|meta| meta.is_file()) {
        return Ok(vec![]);
    }
    let browser_id = match profile.browser.as_str() {
        "Chrome" => "chrome",
        "Chromium" => "chromium",
        "Edge" => "edge",
        "Brave" => "brave",
        _ => "firefox",
    };
    let snapshot = if browser_id == "firefox" {
        None
    } else {
        Some(ChromiumSnapshot::create(&db)?)
    };
    let source = snapshot
        .as_ref()
        .map(|snapshot| snapshot.0.join("Cookies"))
        .unwrap_or(db);
    let mut request = FromPathRequest::new(source)
        .timeout(std::time::Duration::from_secs(8))
        .cancellation(cancellation.clone());
    if browser_id != "firefox" {
        #[cfg(unix)]
        {
            request = request.chromium_browser_id(browser_id);
        }
        #[cfg(windows)]
        if let Some(root) = profile.path.parent() {
            request = request.chromium_local_state(root.join("Local State"));
        }
    }
    // No app-bound bypass feature is compiled. Respect OS protection and skip unsupported rows.
    let result = rookie_cookies::from_path(request.app_bound(AppBoundPolicy::Disabled))
        .map_err(|_| CookieLoadFailure::Unresolved)?;
    let mut entries = result.into_detailed_cookies();
    if snapshot.is_some() {
        for entry in &mut entries {
            entry
                .context
                .top_frame_site_key
                .get_or_insert_with(String::new);
        }
    }
    native_cookies(entries, target, browser_id == "firefox").map_err(Into::into)
}
fn native_cookies(
    entries: Vec<rookie_cookies::enums::DetailedCookie>,
    target: &url::Url,
    firefox: bool,
) -> Result<Vec<crate::accounts::Cookie>, ()> {
    let host = target.host_str().ok_or(())?;
    let cookies = entries
        .into_iter()
        .filter_map(|entry| {
            let c = entry.cookie;
            let domain = c.domain.trim_start_matches('.').to_ascii_lowercase();
            let host_only = !c.domain.starts_with('.');
            if !(host == domain
                || (!host_only && domain.contains('.') && host.ends_with(&format!(".{domain}"))))
            {
                return None;
            }
            let context = entry.context;
            // Missing isolation metadata is not proof that the source is an unpartitioned store.
            if (firefox && context.origin_attributes.is_none())
                || (!firefox && context.top_frame_site_key.is_none())
            {
                return None;
            }
            // Separate browser containers/private stores must never be merged into the default account.
            if context.user_context_id.unwrap_or(0) != 0
                || context.private_browsing_id.unwrap_or(0) != 0
                || context
                    .partition_key
                    .as_ref()
                    .is_some_and(|v| !v.is_empty())
                || context
                    .origin_attributes
                    .as_ref()
                    .is_some_and(|v| !v.is_empty())
            {
                return None;
            }
            let partition_key = context
                .top_frame_site_key
                .filter(|s| !s.is_empty())
                .map(|site| crate::accounts::PartitionKey {
                    top_level_site: site,
                    has_cross_site_ancestor: context.has_cross_site_ancestor.unwrap_or(false),
                });
            Some(crate::accounts::Cookie {
                name: c.name,
                value: c.value,
                domain: c.domain,
                path: c.path,
                secure: c.secure,
                http_only: c.http_only,
                host_only,
                expires: c.expires.map(|v| v as f64),
                same_site: match c.same_site {
                    0 => "no_restriction",
                    1 => "lax",
                    2 => "strict",
                    _ => "unspecified",
                }
                .into(),
                store_id: "native-default".into(),
                partition_key,
            })
        })
        .collect();
    match crate::accounts::normalize(cookies, target) {
        Ok(cookies) => Ok(cookies),
        Err(error) if error.public_code == "auth_required" => Ok(vec![]),
        Err(_) => Err(()),
    }
}
struct ScanPermit(&'static AtomicBool);
impl Drop for ScanPermit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

struct CancelOnDrop(CancellationHandle);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

async fn run_native_scan<T, F, P>(
    scanning: &'static AtomicBool,
    budget: Duration,
    permitted: P,
    work: F,
) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(CancellationHandle) -> T + Send + 'static,
    P: Fn() -> AppResult<()>,
{
    permitted()?;
    if scanning.swap(true, Ordering::SeqCst) {
        return Err(AppError::api("browser_busy", 409));
    }
    let permit = ScanPermit(scanning);
    let cancellation = CancellationHandle::new();
    let _cancel = CancelOnDrop(cancellation.clone());
    let mut scan = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work(cancellation)
    });
    let deadline = tokio::time::sleep(budget);
    tokio::pin!(deadline);
    let mut policy_check = tokio::time::interval(Duration::from_millis(100));
    policy_check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            result = &mut scan => {
                let result = result.map_err(|_| AppError::api("browser_unavailable", 422))?;
                permitted()?;
                return Ok(result);
            }
            _ = &mut deadline => return Err(AppError::api("browser_unavailable", 408)),
            _ = policy_check.tick() => permitted()?,
        }
    }
}

fn with_native_admission<T>(
    root: &Path,
    intent: crate::native_scan_admission::NativeScanIntent,
    cancellation: &CancellationHandle,
    work: impl FnOnce() -> T,
) -> AppResult<T> {
    if cancellation.is_cancelled() {
        return Err(AppError::api("native_discovery_retry_required", 409));
    }
    let admission = crate::native_scan_admission::NativeScanPermit::acquire(root, intent)?;
    let result = work();
    if cancellation.is_cancelled() {
        return Err(AppError::api("native_discovery_retry_required", 409));
    }
    admission.clean_complete()?;
    Ok(result)
}

fn scan_site(
    target: &url::Url,
    wanted_profile: &str,
    profiles: Option<Vec<BrowserProfile>>,
    cancellation: &CancellationHandle,
    permitted: impl Fn() -> bool,
) -> (usize, usize, Vec<ImportedSession>) {
    if cancellation.is_cancelled() || !permitted() {
        cancellation.cancel();
        return (0, 0, vec![]);
    }
    // Local fixture/development servers do not inherit unrelated browser credentials.
    if target
        .host_str()
        .is_none_or(|host| host == "localhost" || host.parse::<std::net::IpAddr>().is_ok())
    {
        return (0, 0, vec![]);
    }
    scan_profiles(
        profiles
            .unwrap_or_else(discover)
            .into_iter()
            .filter(|p| wanted_profile.is_empty() || p.id == wanted_profile)
            .take(24),
        cancellation,
        |profile| {
            read_native_profile(
                profile,
                cancellation,
                &permitted,
                |profile| load_site_cookies(profile, target, cancellation),
                |profile| {
                    crate::browser_storage::load(
                        &profile.path,
                        profile.browser == "Firefox",
                        &target.origin().ascii_serialization(),
                    )
                },
            )
        },
    )
}

fn read_native_profile(
    profile: BrowserProfile,
    cancellation: &CancellationHandle,
    permitted: impl Fn() -> bool,
    load_cookies: impl FnOnce(
        &BrowserProfile,
    ) -> Result<Vec<crate::accounts::Cookie>, CookieLoadFailure>,
    load_storage: impl FnOnce(&BrowserProfile) -> Result<crate::browser_storage::Storage, ()>,
) -> ImportedSession {
    if cancellation.is_cancelled() || !permitted() {
        cancellation.cancel();
        return ImportedSession::from_results(profile, Err(()), Err(()));
    }
    let cookies = load_cookies(&profile);
    if matches!(cookies, Err(CookieLoadFailure::Unresolved)) {
        // Native access errors have no reliable denial discriminator. Do not fan out
        // to another profile or prompt after an unresolved native request.
        cancellation.cancel();
    }
    if cancellation.is_cancelled() || !permitted() {
        cancellation.cancel();
        return ImportedSession::from_results(profile, Err(()), Err(()));
    }
    let storage = load_storage(&profile);
    ImportedSession::from_results(profile, cookies.map_err(|_| ()), storage)
}

fn scan_profiles(
    profiles: impl IntoIterator<Item = BrowserProfile>,
    cancellation: &CancellationHandle,
    mut read: impl FnMut(BrowserProfile) -> ImportedSession,
) -> (usize, usize, Vec<ImportedSession>) {
    let mut checked = 0;
    let mut unavailable = 0;
    let mut found = vec![];
    for profile in profiles {
        if cancellation.is_cancelled() {
            return (checked, unavailable, vec![]);
        }
        checked += 1;
        let session = read(profile);
        if !session.complete {
            unavailable += 1;
        }
        if cancellation.is_cancelled() {
            return (checked, unavailable, vec![]);
        }
        found.push(session);
    }
    (checked, unavailable, found)
}

impl Runtime {
    fn native_discovery_permitted(&self) -> AppResult<()> {
        if self.state.settings()?.native_browser_discovery {
            Ok(())
        } else {
            Err(AppError::api("native_discovery_disabled", 409))
        }
    }

    /// A failed local read is not a logout. Keep the old snapshot whole rather
    /// than mixing cookies and storage that may belong to different sign-ins.
    fn replace_imported_account(&self, id: &str, session: &ImportedSession) -> AppResult<bool> {
        if !session.complete {
            return Ok(false);
        }
        let (credential, changed) = self.update_credential(id, |credential| {
            let changed = serde_json::to_value(&credential.cookies)?
                != serde_json::to_value(&session.cookies)?
                || credential.local_storage != session.storage;
            credential.cookies = session.cookies.clone();
            credential.local_storage = session.storage.clone();
            if changed {
                credential.headers.clear();
            }
            Ok(changed)
        })?;
        self.state.update(|d| {
            if let Some(account) = d.accounts.get_mut(id) {
                account.cookie_count = credential.cookies.len();
                account.status =
                    if credential.cookies.is_empty() && credential.local_storage.is_empty() {
                        "expired"
                    } else {
                        "connected"
                    }
                    .into();
                if changed {
                    account.verified_identity = false;
                    account.label.clear();
                    account.email.clear();
                    account.user_id.clear();
                    account.name.clear();
                }
            }
            Ok(())
        })?;
        Ok(true)
    }
    pub async fn find_browser_accounts(
        self: &std::sync::Arc<Self>,
        site_id: &str,
        raw_url: &str,
        profile_id: &str,
    ) -> AppResult<BrowserSearch> {
        self.import_browser_accounts(
            site_id,
            raw_url,
            profile_id,
            true,
            None,
            crate::native_scan_admission::NativeScanIntent::UserRequested,
        )
        .await
    }
    pub(crate) async fn import_browser_accounts(
        self: &std::sync::Arc<Self>,
        site_id: &str,
        raw_url: &str,
        profile_id: &str,
        render: bool,
        profiles: Option<Vec<BrowserProfile>>,
        intent: crate::native_scan_admission::NativeScanIntent,
    ) -> AppResult<BrowserSearch> {
        let target = crate::net::website_url(raw_url)?;
        if !crate::policy::safe_read_url(&target) {
            return Err(crate::apperr::AppError::api("bad_url", 400));
        }
        let effective_id = if site_id.is_empty() {
            crate::runtime::site_id_for(&target)
        } else {
            if crate::runtime::origin(self.spec(site_id)?.site.source_url())?
                != target.origin().ascii_serialization()
            {
                return Err(crate::apperr::AppError::api("bad_url", 400));
            }
            site_id.into()
        };
        if !self.state.settings()?.native_browser_discovery {
            return Err(crate::apperr::AppError::api(
                "native_discovery_disabled",
                409,
            ));
        }
        let target_copy = target.clone();
        let wanted_profile = profile_id.to_string();
        static SCANNING: AtomicBool = AtomicBool::new(false);
        let policy_core = self.clone();
        let (checked, unavailable, sessions) = run_native_scan(
            &SCANNING,
            Duration::from_secs(300),
            || self.native_discovery_permitted(),
            move |cancellation| {
                policy_core.native_discovery_permitted()?;
                with_native_admission(&policy_core.root, intent, &cancellation, || {
                    scan_site(
                        &target_copy,
                        &wanted_profile,
                        profiles,
                        &cancellation,
                        || policy_core.native_discovery_permitted().is_ok(),
                    )
                })
            },
        )
        .await??;
        for session in sessions {
            self.native_discovery_permitted()?;
            let existing = self
                .state
                .read()?
                .accounts
                .values()
                .filter(|a| a.site_id == effective_id)
                .find_map(|a| {
                    self.credential(&a.id)
                        .ok()
                        .filter(|c| c.source_profile == session.profile.id)
                        .map(|_| a.id.clone())
                });
            if existing.is_none() && session.cookies.is_empty() && session.storage.is_empty() {
                continue;
            }
            let id = if let Some(id) = existing {
                if !self.replace_imported_account(&id, &session)? {
                    continue;
                }
                id
            } else {
                // A not-yet-installed site has the same deterministic ID used by preparation.
                let before = self
                    .state
                    .read()?
                    .sites
                    .get(&effective_id)
                    .map(|m| m.account_id.clone())
                    .unwrap_or_default();
                let account = self.register_browser_credential(
                    &effective_id,
                    crate::accounts::Credential {
                        identity_proof: String::new(),
                        origin: target.origin().ascii_serialization(),
                        cookies: session.cookies,
                        local_storage: session.storage,
                        headers: Default::default(),
                        recipe: None,
                        identity_source: String::new(),
                        browser_session: String::new(),
                        extension_origin: String::new(),
                        browser_session_expires_ms: 0,
                        source_profile: session.profile.id.clone(),
                    },
                    &session.profile.browser,
                    &session.profile.profile,
                )?;
                // Defer selecting until all profiles have been considered.
                self.state.update(|d| {
                    let meta = d.sites.entry(effective_id.clone()).or_default();
                    if meta.account_selection_revision.is_empty() && meta.account_id == account.id {
                        meta.account_id = before;
                    }
                    Ok(())
                })?;
                self.update_credential(&account.id, |credential| {
                    credential.source_profile = session.profile.id;
                    Ok(())
                })?;
                account.id
            };
            // A proven recipe references portable local keys, not the old machine's
            // profile path. Re-resolve it against a newly imported profile and verify.
            let saved_recipe = self
                .state
                .read()?
                .sites
                .get(&effective_id)
                .and_then(|m| m.connection_recipe.clone());
            if self.credential(&id)?.recipe.is_none() {
                if let Some(recipe) = saved_recipe {
                    let _ = self.verify_session_recipe(&id, &recipe).await;
                }
            }
            let refreshed = self.refresh_account(&id).await;
            if render
                && !refreshed.is_ok_and(|view| view.verified_identity && view.status == "connected")
            {
                let credential = self.credential(&id)?;
                if !credential.cookies.is_empty() || !credential.local_storage.is_empty() {
                    let entry = match self.read_page(raw_url, "").await {
                        Ok(page) => published_login(&target, &String::from_utf8_lossy(&page.bytes))
                            .unwrap_or_else(|| raw_url.into()),
                        Err(_) => raw_url.into(),
                    };
                    if let Ok(capture) = crate::browser_session::capture(
                        &credential,
                        &entry,
                        self.network.allow_local,
                    )
                    .await
                    {
                        self.retain_browser_capture(&id, &credential, capture).await?;
                    }
                }
            }
        }
        let selected = self.browser_account_for(&effective_id, raw_url)?;
        let accounts: Vec<_> = self
            .state
            .read()?
            .accounts
            .values()
            .filter(|a| a.site_id == effective_id)
            .cloned()
            .collect();
        self.changed();
        Ok(BrowserSearch {
            profiles_checked: checked,
            profiles_unavailable: unavailable,
            needs_choice: selected.is_empty()
                && accounts
                    .iter()
                    .filter(|a| a.status == "connected" && a.verified_identity)
                    .map(identity_key)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    > 1,
            accounts,
            selected_account: selected,
            login_url: login_url(&target),
        })
    }
    async fn retain_browser_capture(
        &self,
        id: &str,
        expected: &crate::accounts::Credential,
        capture: crate::browser_session::Capture,
    ) -> AppResult<()> {
        let account = self.state.account(id)?.ok_or_else(|| AppError::api("not_found", 404))?;
        self.state.update(|data| {
            let meta = data.sites.entry(account.site_id.clone()).or_default();
            for observation in capture.observations {
                if !meta.observations.iter().any(|old| old.method == observation.method && old.path == observation.path) {
                    meta.observations.push(observation);
                }
            }
            Ok(())
        })?;
        if capture.headers.is_empty() && capture.identity.is_none() {
            return Ok(());
        }
        let mut candidate = expected.clone();
        candidate.headers.extend(capture.headers);
        let has_identity = capture.identity.is_some();
        if has_identity {
            candidate.identity_source = capture.identity_url;
        }
        let expected = self.stage_captured_credential(id, expected, candidate.clone())?;
        if has_identity {
            let _ = self.verify_identity_candidate(id, &expected, candidate, crate::runtime::CandidateIdentityProbe::Capture).await;
        }
        Ok(())
    }

}
pub fn login_url(target: &url::Url) -> String {
    target.to_string()
}
pub fn published_login(target: &url::Url, html: &str) -> Option<String> {
    let links = regex::Regex::new(
        r#"(?is)<a\b[^>]*\bhref\s*=\s*[\"']([^\"']{1,2048})[\"'][^>]*>(.*?)</a>"#,
    )
    .ok()?;
    links
        .captures_iter(html)
        .filter_map(|capture| {
            let url = target.join(&capture[1]).ok()?;
            let hint = format!(
                "{} {}",
                url.path(),
                crate::runtime::visible_text(&capture[2])
            )
            .to_lowercase();
            (url.origin() == target.origin()
                && crate::browser_session::navigation_safe(&url)
                && ["login", "log-in", "signin", "sign-in", "sign in", "log in"]
                    .iter()
                    .any(|word| hint.contains(word)))
            .then(|| url.to_string())
        })
        .next()
}

impl Runtime {
    /// Reuse the user's selected account first. Never silently choose between different accounts.
    pub(crate) fn browser_account_for(&self, site_id: &str, url: &str) -> AppResult<String> {
        if let Some(snapshot) = self.selection_snapshot(site_id, url)? {
            return Ok(snapshot.account_id);
        }
        let target_url = crate::net::validate_url(url)?;
        let data = self.state.read()?;
        let usable = |id: &str| {
            data.accounts.get(id).is_some_and(|account| {
                account.site_id == site_id
                    && crate::net::validate_url(&account.site_url)
                        .is_ok_and(|site_url| crate::net::same_site_origin(&target_url, &site_url))
                    && account.status == "connected"
                    && account.verified_identity
            })
        };
        if let Some(meta) = data.sites.get(site_id) {
            if !meta.account_id.is_empty()
                && data.accounts.get(&meta.account_id).is_some_and(|a| {
                    a.verified_identity || !a.user_id.is_empty() || !a.email.is_empty()
                })
            {
                // An expired selected account must not switch silently to another person.
                return Ok(if usable(&meta.account_id) {
                    meta.account_id.clone()
                } else {
                    String::new()
                });
            }
        }
        let accounts: Vec<_> = data
            .accounts
            .values()
            .filter(|account| usable(&account.id))
            .collect();
        if accounts.is_empty()
            || accounts
                .iter()
                .map(|a| identity_key(a))
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != 1
        {
            return Ok(String::new());
        }
        let id = accounts[0].id.clone();
        self.state.update(|state| {
            let meta = state.sites.entry(site_id.into()).or_default();
            if !meta.account_selection_revision.is_empty() {
                return Err(AppError::api("auth_required", 401));
            }
            meta.account_id = id.clone();
            Ok(())
        })?;
        Ok(id)
    }
}
fn identity_key(account: &crate::accounts::AccountView) -> String {
    if !account.user_id.is_empty() {
        format!("id:{}", account.user_id)
    } else {
        format!("email:{}", account.email.to_lowercase())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct CaptureFixture {
        core: Runtime,
        id: String,
        origin: String,
        saved_status: std::sync::Arc<std::sync::atomic::AtomicU16>,
        candidate_status: std::sync::Arc<std::sync::atomic::AtomicU16>,
        anonymous_status: std::sync::Arc<std::sync::atomic::AtomicU16>,
        server: tokio::task::JoinHandle<()>,
    }

    impl CaptureFixture {
        async fn new(proven: bool) -> Self {
            use axum::{Json, Router, http::{HeaderMap, StatusCode}, routing::get};
            use std::sync::{Arc, atomic::{AtomicU16, Ordering}};
            let saved_status = Arc::new(AtomicU16::new(200));
            let candidate_status = Arc::new(AtomicU16::new(200));
            let anonymous_status = Arc::new(AtomicU16::new(401));
            let saved = saved_status.clone();
            let candidate = candidate_status.clone();
            let anonymous = anonymous_status.clone();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let handler = move |headers: HeaderMap| {
                let status = match headers.get("authorization").and_then(|v| v.to_str().ok()) {
                    Some("synthetic-saved") => saved.load(Ordering::SeqCst),
                    Some("synthetic-candidate") => candidate.load(Ordering::SeqCst),
                    _ => anonymous.load(Ordering::SeqCst),
                };
                async move {
                    (StatusCode::from_u16(status).unwrap(), Json(serde_json::json!({"id":"person", "name":"Person"})))
                }
            };
            let router = Router::new().route("/api/me", get(handler.clone())).route("/read", get(handler));
            let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/tests").join(util::id());
            let core = Runtime::open(&root, None, None, false, true, false).unwrap();
            let credential = crate::accounts::Credential {
                origin: origin.clone(),
                identity_source: format!("{origin}/api/me"),
                headers: [("authorization".into(), "synthetic-saved".into())].into(),
                ..Default::default()
            };
            let account = core.register_browser_credential("fixture", credential.clone(), "Fixture", "Synthetic").unwrap();
            if proven {
                core.publish_verified_identity(&account.id, &credential, credential.clone(), crate::accounts::Identity {
                    user_id: "person".into(), name: "Person".into(), ..Default::default()
                }).unwrap();
            }
            Self { core, id: account.id, origin, saved_status, candidate_status, anonymous_status, server }
        }

        fn capture(&self, identity: bool) -> crate::browser_session::Capture {
            crate::browser_session::Capture {
                headers: [("authorization".into(), "synthetic-candidate".into())].into(),
                identity: identity.then(|| crate::accounts::Identity { user_id: "untrusted-browser-person".into(), ..Default::default() }),
                identity_url: format!("{}/api/me", self.origin),
                ..Default::default()
            }
        }

        fn snapshot(&self) -> serde_json::Value {
            let state = self.core.state.read().unwrap();
            serde_json::json!({
                "credential": self.core.credential(&self.id).unwrap(),
                "account": state.accounts[&self.id],
                "generation": state.identity_generations.get(&self.id),
                "selection": state.sites["fixture"].account_id,
            })
        }

        async fn assert_usable(&self) {
            assert_eq!(self.core.browser_account_for("fixture", &self.origin).unwrap(), self.id);
            assert!(self.core.read_page(&format!("{}/read", self.origin), &self.id).await.unwrap().successful());
        }
    }

    impl Drop for CaptureFixture {
        fn drop(&mut self) {
            self.server.abort();
            let _ = std::fs::remove_dir_all(&self.core.root);
        }
    }

    #[tokio::test]
    async fn failed_capture_after_transient_refresh_preserves_proof_and_usable_account() {
        use std::sync::atomic::Ordering;
        let fixture = CaptureFixture::new(true).await;
        let before = fixture.snapshot();
        fixture.saved_status.store(503, Ordering::SeqCst);
        assert!(fixture.core.refresh_account(&fixture.id).await.is_err());
        assert_eq!(fixture.snapshot(), before);
        fixture.saved_status.store(200, Ordering::SeqCst);
        for status in [503, 401] {
            fixture.candidate_status.store(status, Ordering::SeqCst);
            let expected = fixture.core.credential(&fixture.id).unwrap();
            fixture.core.retain_browser_capture(&fixture.id, &expected, fixture.capture(true)).await.unwrap();
            assert_eq!(fixture.snapshot(), before);
            fixture.assert_usable().await;
        }
        fixture.saved_status.store(401, Ordering::SeqCst);
        let expired = fixture.core.refresh_account(&fixture.id).await.unwrap();
        assert_eq!(expired.status, "expired");
        assert!(!expired.verified_identity);
        assert!(fixture.core.credential(&fixture.id).unwrap().identity_proof.is_empty());
    }

    #[tokio::test]
    async fn header_only_capture_remains_available_to_recipe_discovery_without_replacing_proof() {
        let fixture = CaptureFixture::new(true).await;
        let before = fixture.snapshot();
        let expected = fixture.core.credential(&fixture.id).unwrap();
        let mut capture = fixture.capture(false);
        capture.headers.insert("x-session".into(), "synthetic-private-pending".into());
        fixture.core.retain_browser_capture(&fixture.id, &expected, capture).await.unwrap();
        assert_eq!(fixture.snapshot(), before);
        fixture.assert_usable().await;
        let inspected = fixture.core.inspect_account_session(&fixture.id).unwrap();
        assert!(inspected["captured_headers"].as_array().unwrap().iter().any(|name| name == "x-session"));
        assert!(!inspected.to_string().contains("synthetic-private-pending"));
        {
            let vault = fixture.core.vault.lock().unwrap();
            assert!(!vault.keys().iter().any(|key| key.starts_with("@capture/")));
            assert!(crate::accounts::collect_redaction_secrets(&vault).contains(&"synthetic-private-pending".into()));
        }
        let recipe = crate::session_recipe::ConnectionRecipe {
            identity_url: format!("{}/api/me", fixture.origin),
            headers: vec![crate::session_recipe::HeaderSource {
                header: "authorization".into(), source: "captured_header".into(), key: "authorization".into(), ..Default::default()
            }],
            identity: crate::session_recipe::IdentityMapping { user_id: "/id".into(), name: "/name".into(), ..Default::default() },
            evidence: "observed_browser_request".into(),
            ..Default::default()
        };
        let published = fixture.core.verify_session_recipe(&fixture.id, &recipe).await.unwrap();
        assert!(published.verified_identity);
        let credential = fixture.core.credential(&fixture.id).unwrap();
        assert_eq!(credential.headers["authorization"], "synthetic-candidate");
        assert_ne!(credential.identity_proof, expected.identity_proof);
        let state = fixture.core.state.read().unwrap();
        assert!(credential.proves_identity(&published, &state.identity_generations[&fixture.id]).unwrap());
        assert!(fixture.core.vault.lock().unwrap().get("@capture", &fixture.id).is_none());
    }

    #[tokio::test]
    async fn saved_session_refresh_does_not_consume_pending_capture_headers() {
        use std::sync::atomic::Ordering;
        let fixture = CaptureFixture::new(true).await;
        let recipe = crate::session_recipe::ConnectionRecipe {
            identity_url: format!("{}/api/me", fixture.origin),
            headers: vec![crate::session_recipe::HeaderSource {
                header: "authorization".into(), source: "captured_header".into(), key: "authorization".into(), ..Default::default()
            }],
            identity: crate::session_recipe::IdentityMapping { user_id: "/id".into(), name: "/name".into(), ..Default::default() },
            evidence: "observed_browser_request".into(),
            ..Default::default()
        };
        fixture.core.verify_session_recipe(&fixture.id, &recipe).await.unwrap();
        let expected = fixture.core.credential(&fixture.id).unwrap();
        let mut capture = fixture.capture(false);
        capture.observations.push(crate::accounts::Observation { method: "GET".into(), path: "/api/me".into(), ..Default::default() });
        fixture.core.retain_browser_capture(&fixture.id, &expected, capture).await.unwrap();
        fixture.candidate_status.store(401, Ordering::SeqCst);
        let before = fixture.snapshot();
        assert_eq!(fixture.core.verify_observed_session(&fixture.id, &recipe).await.unwrap_err().public_code, "auth_required");
        assert_eq!(fixture.snapshot(), before);
        fixture.assert_usable().await;
        fixture.core.refresh_account_session(&fixture.id).await.unwrap();
        assert_eq!(fixture.core.credential(&fixture.id).unwrap().headers["authorization"], "synthetic-saved");
        fixture.saved_status.store(401, Ordering::SeqCst);
        assert_eq!(fixture.core.refresh_account_session(&fixture.id).await.unwrap_err().public_code, "auth_required");
        assert!(!fixture.core.state.account(&fixture.id).unwrap().unwrap().verified_identity);
    }

    #[tokio::test]
    async fn successful_capture_recovers_a_paused_saved_account_only_after_publication() {
        use std::sync::atomic::Ordering;
        let fixture = CaptureFixture::new(true).await;
        fixture.saved_status.store(401, Ordering::SeqCst);
        let denied = fixture.core.read_page(&format!("{}/read", fixture.origin), &fixture.id).await.unwrap();
        assert_eq!(denied.status, 401);
        fixture.saved_status.store(200, Ordering::SeqCst);
        let expected = fixture.core.credential(&fixture.id).unwrap();
        fixture.core.retain_browser_capture(&fixture.id, &expected, fixture.capture(true)).await.unwrap();
        let current = fixture.core.credential(&fixture.id).unwrap();
        assert_eq!(current.headers["authorization"], "synthetic-candidate");
        assert_ne!(current.identity_proof, expected.identity_proof);
        assert_eq!(fixture.core.state.account(&fixture.id).unwrap().unwrap().user_id, "person");
        fixture.assert_usable().await;
    }

    #[tokio::test]
    async fn header_only_capture_for_unverified_account_retains_headers_without_identity() {
        let fixture = CaptureFixture::new(false).await;
        let expected = fixture.core.credential(&fixture.id).unwrap();
        fixture.core.retain_browser_capture(&fixture.id, &expected, fixture.capture(false)).await.unwrap();
        let credential = fixture.core.credential(&fixture.id).unwrap();
        assert_eq!(credential.headers["authorization"], "synthetic-candidate");
        assert!(credential.identity_proof.is_empty());
        let account = fixture.core.state.account(&fixture.id).unwrap().unwrap();
        assert!(!account.verified_identity);
        assert!(account.user_id.is_empty());
    }

    #[tokio::test]
    async fn inconclusive_anonymous_capture_probe_and_observation_only_capture_preserve_proof() {
        use std::sync::atomic::Ordering;
        let fixture = CaptureFixture::new(true).await;
        let before = fixture.snapshot();
        let expected = fixture.core.credential(&fixture.id).unwrap();
        for status in [503, 302, 429] {
            fixture.anonymous_status.store(status, Ordering::SeqCst);
            fixture.core.retain_browser_capture(&fixture.id, &expected, fixture.capture(true)).await.unwrap();
            assert_eq!(fixture.snapshot(), before);
            if status != 429 {
                fixture.assert_usable().await;
            }
        }
        fixture.core.retain_browser_capture(&fixture.id, &expected, crate::browser_session::Capture {
            observations: vec![crate::accounts::Observation { method: "GET".into(), path: "/observed".into(), ..Default::default() }],
            ..Default::default()
        }).await.unwrap();
        assert_eq!(fixture.snapshot(), before);
        assert!(fixture.core.state.read().unwrap().sites["fixture"].observations.iter().any(|o| o.path == "/observed"));
    }

    #[tokio::test]
    async fn removing_account_credentials_removes_private_pending_capture() {
        let fixture = CaptureFixture::new(true).await;
        let expected = fixture.core.credential(&fixture.id).unwrap();
        fixture.core.retain_browser_capture(&fixture.id, &expected, fixture.capture(false)).await.unwrap();
        let mut vault = fixture.core.vault.lock().unwrap();
        assert!(vault.get("@capture", &fixture.id).is_some());
        vault.delete("accounts", &fixture.id).unwrap();
        assert!(vault.get("accounts", &fixture.id).is_none());
        assert!(vault.get("@capture", &fixture.id).is_none());
    }

    #[tokio::test]
    async fn stale_capture_cannot_replace_newer_proof_or_supply_recipe_headers() {
        let fixture = CaptureFixture::new(true).await;
        let expected = fixture.core.credential(&fixture.id).unwrap();
        fixture.core.retain_browser_capture(&fixture.id, &expected, fixture.capture(false)).await.unwrap();
        let candidate = fixture.core.capture_candidate(&fixture.id, &expected).unwrap();
        fixture.core.refresh_account(&fixture.id).await.unwrap();
        let before = fixture.snapshot();
        let current = fixture.core.credential(&fixture.id).unwrap();
        assert_eq!(fixture.core.capture_candidate(&fixture.id, &current).unwrap().headers["authorization"], "synthetic-saved");
        assert_eq!(fixture.core.retain_browser_capture(&fixture.id, &expected, fixture.capture(false)).await.unwrap_err().public_code, "conflict");
        assert_eq!(fixture.core.verify_identity_candidate(&fixture.id, &expected, candidate, crate::runtime::CandidateIdentityProbe::Capture).await.unwrap_err().public_code, "conflict");
        assert_eq!(fixture.snapshot(), before);
        fixture.assert_usable().await;
    }

    #[test]
    fn native_scan_cancellation_stops_before_another_profile_and_discards_partial_result() {
        let cancellation = CancellationHandle::new();
        let mut calls = 0;
        let (checked, _, sessions) = scan_profiles(
            (0..4).map(|_| BrowserProfile::default()),
            &cancellation,
            |profile| {
                calls += 1;
                cancellation.cancel();
                ImportedSession::from_results(profile, Ok(vec![]), Ok(Default::default()))
            },
        );
        assert_eq!(calls, 1, "cancellation must stop further profile work");
        assert_eq!(checked, 1);
        assert!(sessions.is_empty(), "cancelled work must not be published");
    }

    #[test]
    fn native_scan_already_cancelled_never_reads_profiles() {
        let cancellation = CancellationHandle::new();
        cancellation.cancel();
        let (checked, unavailable, sessions) =
            scan_profiles([BrowserProfile::default()], &cancellation, |_| {
                panic!("cancelled scan must never read a profile")
            });
        assert_eq!((checked, unavailable), (0, 0));
        assert!(sessions.is_empty());
    }

    #[test]
    fn native_scan_unresolved_native_read_stops_profile_fanout_and_storage() {
        let cancellation = CancellationHandle::new();
        let mut reads = 0;
        let (checked, _, sessions) = scan_profiles(
            (0..3).map(|_| BrowserProfile::default()),
            &cancellation,
            |profile| {
                read_native_profile(
                    profile,
                    &cancellation,
                    || true,
                    |_| {
                        reads += 1;
                        Err(CookieLoadFailure::Unresolved)
                    },
                    |_| panic!("unresolved native access must not continue to storage"),
                )
            },
        );
        assert_eq!(reads, 1);
        assert_eq!(checked, 1);
        assert!(sessions.is_empty());
        assert!(cancellation.is_cancelled());
    }

    #[test]
    fn native_scan_unresolved_result_requires_explicit_clean_retry() {
        use crate::native_scan_admission::{NativeScanIntent, NativeScanPermit};
        let root =
            std::env::temp_dir().join(format!("hycli-native-unresolved-{}", crate::util::id()));
        let cancellation = CancellationHandle::new();
        let mut calls = 0;
        let result =
            with_native_admission(&root, NativeScanIntent::Preparation, &cancellation, || {
                scan_profiles(
                    (0..3).map(|_| BrowserProfile::default()),
                    &cancellation,
                    |profile| {
                        read_native_profile(
                            profile,
                            &cancellation,
                            || true,
                            |_| {
                                calls += 1;
                                Err(CookieLoadFailure::Unresolved)
                            },
                            |_| panic!("unresolved native read must not continue to storage"),
                        )
                    },
                )
            });
        assert_eq!(
            result.err().unwrap().public_code,
            "native_discovery_retry_required"
        );
        assert_eq!(calls, 1);
        assert_eq!(
            NativeScanPermit::acquire(&root, NativeScanIntent::Preparation)
                .err()
                .unwrap()
                .public_code,
            "native_discovery_retry_required"
        );
        with_native_admission(
            &root,
            NativeScanIntent::UserRequested,
            &CancellationHandle::new(),
            || (),
        )
        .unwrap();
        NativeScanPermit::acquire(&root, NativeScanIntent::Preparation)
            .unwrap()
            .clean_complete()
            .unwrap();
    }

    #[test]
    fn native_scan_local_unavailable_does_not_trip_native_retry_breaker() {
        let cancellation = CancellationHandle::new();
        let (checked, unavailable, sessions) = scan_profiles(
            (0..2).map(|_| BrowserProfile::default()),
            &cancellation,
            |profile| {
                read_native_profile(
                    profile,
                    &cancellation,
                    || true,
                    |_| Err(CookieLoadFailure::Unavailable),
                    |_| Ok(Default::default()),
                )
            },
        );
        assert_eq!((checked, unavailable), (2, 2));
        assert_eq!(sessions.len(), 2);
        assert!(!cancellation.is_cancelled());
    }

    #[tokio::test]
    async fn native_scan_caller_abort_cancels_owned_worker_without_releasing_gate() {
        static GATE: AtomicBool = AtomicBool::new(false);
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let (finished, done) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(run_native_scan(
            &GATE,
            Duration::from_secs(10),
            || Ok(()),
            move |cancel| {
                started.send(cancel.clone()).unwrap();
                wait.recv().unwrap();
                finished.send(()).unwrap();
            },
        ));
        let cancellation = ready.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(
            GATE.load(Ordering::SeqCst),
            "worker still owns the scan permit"
        );
        let second = run_native_scan(&GATE, Duration::from_secs(1), || Ok(()), |_| ()).await;
        assert_eq!(second.unwrap_err().public_code, "browser_busy");
        release.send(()).unwrap();
        done.await.unwrap();
        assert!(
            cancellation.is_cancelled(),
            "dropping the caller must cancel its native request"
        );
    }

    #[tokio::test]
    async fn native_scan_timeout_cancels_owned_worker() {
        static GATE: AtomicBool = AtomicBool::new(false);
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let (finished, done) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(run_native_scan(
            &GATE,
            Duration::from_millis(100),
            || Ok(()),
            move |cancel| {
                started.send(cancel.clone()).unwrap();
                wait.recv().unwrap();
                finished.send(()).unwrap();
            },
        ));
        let cancellation = ready.await.unwrap();
        assert_eq!(task.await.unwrap().unwrap_err().http_status, 408);
        assert!(
            GATE.load(Ordering::SeqCst),
            "timeout must not release a running worker's permit"
        );
        release.send(()).unwrap();
        done.await.unwrap();
        assert!(
            cancellation.is_cancelled(),
            "timeout must cancel its native request"
        );
    }

    #[tokio::test]
    async fn native_scan_http_disconnect_cancels_worker_without_releasing_admission() {
        use crate::native_scan_admission::{NativeScanIntent, NativeScanPermit};
        use tokio::io::AsyncWriteExt;
        static GATE: AtomicBool = AtomicBool::new(false);
        let root = std::env::temp_dir().join(format!("hycli-disconnect-{}", util::id()));
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let (finished, done) = tokio::sync::oneshot::channel();
        let fixture = std::sync::Arc::new(std::sync::Mutex::new(Some((started, wait, finished))));
        let worker_root = root.clone();
        let app = axum::Router::new().route(
            "/synthetic-scan",
            axum::routing::get(move || {
                let fixture = fixture.clone();
                let worker_root = worker_root.clone();
                async move {
                    let (started, wait, finished) = fixture.lock().unwrap().take().unwrap();
                    let result = run_native_scan(
                        &GATE,
                        Duration::from_secs(10),
                        || Ok(()),
                        move |cancel| {
                            let mut reads = 0;
                            let mut published = 0;
                            let result = with_native_admission(
                                &worker_root,
                                NativeScanIntent::Preparation,
                                &cancel,
                                || {
                                    let mut started = Some(started);
                                    let (_, _, sessions) = scan_profiles(
                                        (0..2).map(|_| BrowserProfile::default()),
                                        &cancel,
                                        |profile| {
                                            read_native_profile(
                                                profile,
                                                &cancel,
                                                || true,
                                                |_| {
                                                    reads += 1;
                                                    if let Some(started) = started.take() {
                                                        started.send(cancel.clone()).unwrap();
                                                        wait.recv().unwrap();
                                                    }
                                                    Ok(vec![])
                                                },
                                                |_| panic!("disconnected caller must not continue to storage"),
                                            )
                                        },
                                    );
                                    published = sessions.len();
                                },
                            );
                            finished
                                .send((reads, published, result.err().map(|e| e.public_code)))
                                .unwrap();
                        },
                    )
                    .await;
                    if result.is_ok() {
                        axum::http::StatusCode::OK
                    } else {
                        axum::http::StatusCode::CONFLICT
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let mut connection = tokio::net::TcpStream::connect(address).await.unwrap();
        connection
            .write_all(
                b"GET /synthetic-scan HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        let cancellation = tokio::time::timeout(Duration::from_secs(2), ready)
            .await
            .unwrap()
            .unwrap();
        drop(connection);
        let cancelled = tokio::time::timeout(Duration::from_secs(2), async {
            while !cancellation.is_cancelled() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .is_ok();
        let held = NativeScanPermit::acquire(&root, NativeScanIntent::UserRequested)
            .err()
            .map(|e| e.public_code);
        let gate_held = GATE.load(Ordering::SeqCst);
        release.send(()).unwrap();
        let outcome = tokio::time::timeout(Duration::from_secs(2), done).await;
        server.abort();
        assert!(
            cancelled,
            "HTTP disconnect must drop the caller and cancel native work"
        );
        assert_eq!(held.as_deref(), Some("browser_busy"));
        assert!(gate_held);
        assert_eq!(
            outcome.unwrap().unwrap(),
            (1, 0, Some("native_discovery_retry_required".into()))
        );
        assert_eq!(
            NativeScanPermit::acquire(&root, NativeScanIntent::Preparation)
                .err()
                .unwrap()
                .public_code,
            "native_discovery_retry_required"
        );
    }

    #[tokio::test]
    async fn native_scan_policy_revocation_cancels_pending_worker() {
        static GATE: AtomicBool = AtomicBool::new(false);
        let root = std::env::temp_dir().join(format!("hycli-native-policy-{}", crate::util::id()));
        let core = Runtime::open(&root, None, None, false, true, false).unwrap();
        core.state
            .update(|data| {
                data.settings.native_browser_discovery = true;
                Ok(())
            })
            .unwrap();
        let external = crate::state::StateStore::open(&root.join("dashboard.json")).unwrap();
        let policy = core.clone();
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let (finished, done) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(run_native_scan(
            &GATE,
            Duration::from_secs(10),
            move || policy.native_discovery_permitted(),
            move |cancel| {
                started.send(cancel.clone()).unwrap();
                wait.recv().unwrap();
                finished.send(()).unwrap();
            },
        ));
        let cancellation = ready.await.unwrap();
        external
            .update(|data| {
                data.settings.native_browser_discovery = false;
                Ok(())
            })
            .unwrap();
        let result = tokio::time::timeout(Duration::from_secs(2), task).await;
        let cancelled = cancellation.is_cancelled();
        let still_running = GATE.load(Ordering::SeqCst);
        release.send(()).unwrap();
        done.await.unwrap();
        assert_eq!(
            result.unwrap().unwrap().unwrap_err().public_code,
            "native_discovery_disabled"
        );
        assert!(cancelled);
        assert!(still_running);
    }

    #[tokio::test]
    async fn native_scan_disabled_policy_never_starts_worker() {
        static GATE: AtomicBool = AtomicBool::new(false);
        let result = run_native_scan(
            &GATE,
            Duration::from_secs(1),
            || Err(AppError::api("native_discovery_disabled", 409)),
            |_| panic!("disabled policy must not start native work"),
        )
        .await;
        assert_eq!(result.unwrap_err().public_code, "native_discovery_disabled");
        assert!(!GATE.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn native_scan_completion_releases_gate_and_preserves_result() {
        static GATE: AtomicBool = AtomicBool::new(false);
        assert_eq!(
            run_native_scan(&GATE, Duration::from_secs(1), || Ok(()), |_| 7)
                .await
                .unwrap(),
            7
        );
        assert!(!GATE.load(Ordering::SeqCst));
        assert_eq!(
            run_native_scan(&GATE, Duration::from_secs(1), || Ok(()), |_| 9)
                .await
                .unwrap(),
            9
        );
    }

    #[test]
    fn native_reimport_read_failures_preserve_the_whole_verified_session() {
        use crate::accounts::{Credential, Identity};
        use serde_json::json;
        let root = std::env::var_os("HYCLI_TEST_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/tests"))
            .join(util::id());
        let core = Runtime::open(&root, None, None, false, true, false).unwrap();
        let credential: Credential = serde_json::from_value(json!({
            "origin":"https://example.com",
            "source_profile":"synthetic-profile",
            "cookies":[{"name":"sid","value":"synthetic-cookie","domain":"example.com","path":"/"}],
            "local_storage":{"session":"synthetic-storage"},
            "headers":{"authorization":"Bearer synthetic-header"}
        }))
        .unwrap();
        let account = core
            .register_browser_credential("fixture", credential.clone(), "Fixture", "Synthetic")
            .unwrap();
        core.publish_verified_identity(
            &account.id,
            &credential,
            credential.clone(),
            Identity {
                user_id: "original-person".into(),
                name: "Original Person".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let profile = || BrowserProfile {
            id: "synthetic-profile".into(),
            ..Default::default()
        };
        let snapshot = || {
            let state = core.state.read().unwrap();
            (
                serde_json::to_value(core.credential(&account.id).unwrap()).unwrap(),
                serde_json::to_value(&state).unwrap(),
                state.identity_generations.clone(),
                state.sites["fixture"].account_selection_revision.clone(),
                serde_json::to_value(&state.sites["fixture"].account_binding).unwrap(),
            )
        };
        let before = snapshot();
        // Exercise real readers using isolated corrupt fixtures, never installed browsers.
        for cookie_failure in [true, false] {
            let path = root.join(if cookie_failure {
                "broken-cookies"
            } else {
                "broken-storage"
            });
            let broken = if cookie_failure {
                path.join("cookies.sqlite")
            } else {
                path.join("storage/default/https+++example.com/ls/data.sqlite")
            };
            std::fs::create_dir_all(broken.parent().unwrap()).unwrap();
            std::fs::write(&broken, b"not a sqlite database").unwrap();
            let cancellation = CancellationHandle::new();
            let (checked, unavailable, sessions) = scan_site(
                &url::Url::parse("https://example.com/").unwrap(),
                "",
                Some(vec![BrowserProfile {
                    path,
                    browser: "Firefox".into(),
                    ..profile()
                }]),
                &cancellation,
                || true,
            );
            assert_eq!((checked, unavailable), (1, 1));
            let accepted = if cookie_failure {
                assert!(
                    cancellation.is_cancelled(),
                    "unresolved native read stops the entire scan"
                );
                assert!(
                    sessions.is_empty(),
                    "unresolved native reads must not publish partial snapshots"
                );
                false
            } else {
                assert!(!cancellation.is_cancelled());
                assert_eq!(sessions.len(), 1);
                core.replace_imported_account(&account.id, &sessions[0])
                    .unwrap()
            };
            assert_eq!(
                snapshot(),
                before,
                "reader failure must preserve the existing session"
            );
            assert!(!accepted);
        }
        let different_storage =
            std::collections::BTreeMap::from([("session".into(), "different-person".into())]);
        for (cookies, storage) in [
            (Err(()), Ok(Default::default())),
            (Err(()), Ok(different_storage.clone())),
            (Ok(credential.cookies.clone()), Err(())),
            (Ok(vec![]), Err(())),
            (Err(()), Err(())),
        ] {
            let imported = ImportedSession::from_results(profile(), cookies, storage);
            assert!(!imported.complete);
            let accepted = core
                .replace_imported_account(&account.id, &imported)
                .unwrap();
            assert_eq!(
                snapshot(),
                before,
                "local read failure must not replace any part of the session"
            );
            assert!(
                !accepted,
                "failed snapshot must skip refresh as well as replacement"
            );
        }
        // The next complete scan still works, without borrowing values from the old person.
        let replacement =
            ImportedSession::from_results(profile(), Ok(vec![]), Ok(different_storage));
        assert!(
            core.replace_imported_account(&account.id, &replacement)
                .unwrap()
        );
        let current = core.credential(&account.id).unwrap();
        assert!(current.cookies.is_empty());
        assert!(current.headers.is_empty());
        assert!(current.identity_proof.is_empty());
        assert_eq!(current.local_storage["session"], "different-person");
        let view = core.state.account(&account.id).unwrap().unwrap();
        assert!(!view.verified_identity);
        assert!(view.user_id.is_empty());
        assert_eq!(view.status, "connected");
        // Authoritative empty results are different from an unreadable profile.
        let empty = ImportedSession::from_results(profile(), Ok(vec![]), Ok(Default::default()));
        assert!(core.replace_imported_account(&account.id, &empty).unwrap());
        assert_eq!(
            core.state.account(&account.id).unwrap().unwrap().status,
            "expired"
        );
        drop(core);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn chromium_partition_projection_requires_known_source_metadata() {
        let database = rusqlite::Connection::open_in_memory().unwrap();
        database.execute_batch("CREATE TABLE cookies(top_frame_site_key, has_cross_site_ancestor); INSERT INTO cookies VALUES ('', 0), ('https://example.com', 1);").unwrap();
        assert!(chromium_isolation_known(&database).is_ok());
        database
            .execute("INSERT INTO cookies VALUES (NULL, 0)", [])
            .unwrap();
        assert!(chromium_isolation_known(&database).is_err());
        database
            .execute("DELETE FROM cookies WHERE top_frame_site_key IS NULL", [])
            .unwrap();
        database
            .execute("INSERT INTO cookies VALUES ('', NULL)", [])
            .unwrap();
        assert!(chromium_isolation_known(&database).is_err());
    }
    fn native(domain: &str) -> rookie_cookies::enums::DetailedCookie {
        serde_json::from_value(serde_json::json!({"cookie":{"domain":domain,"path":"/","secure":true,"expires":4102444800u64,"name":"sid","value":"synthetic-private-cookie","http_only":true,"same_site":1},"context":{"top_frame_site_key":"","has_cross_site_ancestor":false}})).unwrap()
    }
    #[test]
    fn firefox_uses_registered_profiles_and_ignores_backup_copies() {
        let root = std::env::var_os("HYCLI_TEST_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/tests"))
            .join(util::id());
        std::fs::create_dir_all(root.join("active.default-release")).unwrap();
        std::fs::create_dir_all(root.join("active.default-release-backup")).unwrap();
        std::fs::write(
            root.join("profiles.ini"),
            "[Profile0]\nName=default\nIsRelative=1\nPath=active.default-release\n",
        )
        .unwrap();
        let paths = firefox_profiles(&root).unwrap();
        let profiles: Vec<_> = paths
            .into_iter()
            .map(|path| BrowserProfile {
                id: util::id(),
                browser: "Firefox".into(),
                profile: path.file_name().unwrap().to_string_lossy().into_owned(),
                path,
                ..Default::default()
            })
            .collect();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].profile, "active.default-release");
        let public = serde_json::to_string(&profiles).unwrap();
        assert!(!public.contains(&root.to_string_lossy().to_string()));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn native_import_keeps_site_and_partition_boundaries() {
        let target = url::Url::parse("https://app.example.com/").unwrap();
        let mut container = native(".example.com");
        container.context.user_context_id = Some(2);
        let mut private = native(".example.com");
        private.context.private_browsing_id = Some(1);
        let mut cross_site = native(".example.com");
        cross_site.context.top_frame_site_key = Some("https://unrelated.example/".into());
        let mut cross_ancestor = native(".example.com");
        cross_ancestor.context.top_frame_site_key = Some("https://example.com".into());
        cross_ancestor.context.has_cross_site_ancestor = Some(true);
        let cookies = native_cookies(
            vec![
                native(".example.com"),
                native("example.com"),
                native("attackerexample.com"),
                native("unrelated.example"),
                container,
                private,
                cross_site,
                cross_ancestor,
            ],
            &target,
            false,
        )
        .unwrap();
        assert_eq!(cookies.len(), 1);
        assert!(!cookies[0].host_only);
        assert_eq!(cookies[0].domain, "example.com");
        assert_eq!(cookies[0].same_site, "lax");
    }
    #[test]
    fn login_links_must_be_published_same_origin_reads() {
        let target = url::Url::parse("https://example.com/").unwrap();
        assert_eq!(published_login(&target, r#"<a href="https://evil.example/login">Sign in</a><a href="/logout">Login again</a><a href="/members/sign-in">Sign in</a>"#).as_deref(), Some("https://example.com/members/sign-in"));
        assert_eq!(
            login_url(&url::Url::parse("https://discord.com/channels/@me").unwrap()),
            "https://discord.com/channels/@me"
        );
        assert!(published_login(&target, r#"<a href="javascript:alert(1)">Sign in</a>"#).is_none());
    }
    #[test]
    fn native_search_does_not_inspect_browsers_for_local_fixtures() {
        let (checked, unavailable, found) = scan_site(
            &url::Url::parse("http://127.0.0.1:1234/").unwrap(),
            "",
            None,
            &CancellationHandle::new(),
            || true,
        );
        assert_eq!(checked, 0);
        assert_eq!(unavailable, 0);
        assert!(found.is_empty());
    }
}

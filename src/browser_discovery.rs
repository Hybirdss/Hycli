//! Browser inventory is metadata only. Site-scoped session import stays inside the local broker.
use crate::{apperr::AppResult, runtime::Runtime, util};
use serde::Serialize;
use std::path::{Path, PathBuf};
#[derive(Clone, Default, Serialize)]
pub struct BrowserProfile {
    pub id: String,
    pub browser: String,
    pub profile: String,
    pub running: bool,
    pub last_modified_ms: u64,
    #[serde(skip)]
    pub(crate) path: PathBuf,
}
pub(crate) fn firefox_profiles(root: &Path) -> Option<Vec<PathBuf>> {
    let ini = [
        root.join("profiles.ini"),
        root.parent()?.join("profiles.ini"),
    ]
    .into_iter()
    .find(|path| path.is_file())?;
    let text = String::from_utf8(util::read_bounded(&ini, 64 * 1024).ok()?).ok()?;
    let mut profiles = vec![];
    let mut profile = false;
    let mut path = String::new();
    let mut relative = true;
    let mut finish = |profile: bool, path: &str, relative: bool| {
        if profile && !path.is_empty() {
            profiles.push(if relative {
                ini.parent().unwrap().join(path)
            } else {
                PathBuf::from(path)
            });
        }
    };
    for line in text.lines().chain(std::iter::once("[End]")) {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            finish(profile, &path, relative);
            path.clear();
            relative = true;
            profile = line.starts_with("[Profile");
        } else if let Some(value) = line.strip_prefix("Path=") {
            path = value.trim().into();
        } else if let Some(value) = line.strip_prefix("IsRelative=") {
            relative = value.trim() != "0";
        }
    }
    Some(profiles)
}
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
}
struct ChromiumSnapshot(PathBuf);
impl Drop for ChromiumSnapshot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
impl ChromiumSnapshot {
    fn create(source: &Path) -> Result<Self, ()> {
        let snapshot = Self(std::env::temp_dir().join(format!("hycli-browser-{}", util::id())));
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
fn load_site_cookies(
    profile: &BrowserProfile,
    target: &url::Url,
) -> Result<Vec<crate::accounts::Cookie>, ()> {
    use rookie_cookies::{AppBoundPolicy, FromPathRequest};
    let db = if profile.browser == "Firefox" {
        profile.path.join("cookies.sqlite")
    } else if profile.path.join("Network/Cookies").is_file() {
        profile.path.join("Network/Cookies")
    } else {
        profile.path.join("Cookies")
    };
    if !db.is_file() {
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
    let mut request = FromPathRequest::new(source).timeout(std::time::Duration::from_secs(8));
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
    let result =
        rookie_cookies::from_path(request.app_bound(AppBoundPolicy::Disabled)).map_err(|_| ())?;
    let mut entries = result.into_detailed_cookies();
    if snapshot.is_some() {
        for entry in &mut entries {
            entry
                .context
                .top_frame_site_key
                .get_or_insert_with(String::new);
        }
    }
    native_cookies(entries, target, browser_id == "firefox")
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
fn scan_site(
    target: &url::Url,
    wanted_profile: &str,
    profiles: Option<Vec<BrowserProfile>>,
) -> (usize, usize, Vec<ImportedSession>) {
    let mut checked = 0;
    let mut unavailable = 0;
    let mut found = vec![];
    // Local fixture/development servers do not inherit unrelated browser credentials.
    if target
        .host_str()
        .is_none_or(|host| host == "localhost" || host.parse::<std::net::IpAddr>().is_ok())
    {
        return (0, 0, vec![]);
    }
    for profile in profiles
        .unwrap_or_else(discover)
        .into_iter()
        .filter(|p| wanted_profile.is_empty() || p.id == wanted_profile)
        .take(24)
    {
        checked += 1;
        let cookies = load_site_cookies(&profile, target);
        let storage = crate::browser_storage::load(
            &profile.path,
            profile.browser == "Firefox",
            &target.origin().ascii_serialization(),
        );
        if cookies.is_err() && !storage.as_ref().is_ok_and(|s| !s.is_empty()) {
            unavailable += 1;
        }
        found.push(ImportedSession {
            profile,
            cookies: cookies.unwrap_or_default(),
            storage: storage.unwrap_or_default(),
        });
    }
    (checked, unavailable, found)
}
impl Runtime {
    pub async fn find_browser_accounts(
        self: &std::sync::Arc<Self>,
        site_id: &str,
        raw_url: &str,
        profile_id: &str,
    ) -> AppResult<BrowserSearch> {
        self.import_browser_accounts(site_id, raw_url, profile_id, true, None)
            .await
    }
    pub(crate) async fn import_browser_accounts(
        self: &std::sync::Arc<Self>,
        site_id: &str,
        raw_url: &str,
        profile_id: &str,
        render: bool,
        profiles: Option<Vec<BrowserProfile>>,
    ) -> AppResult<BrowserSearch> {
        let target = crate::net::validate_url(raw_url)?;
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
        let target_copy = target.clone();
        let wanted_profile = profile_id.to_string();
        // Serialise native keyring access. A timeout does not start competing background scans.
        static SCANNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if SCANNING.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return Err(crate::apperr::AppError::api("browser_busy", 409));
        }
        let scan = tokio::task::spawn_blocking(move || {
            struct Release;
            impl Drop for Release {
                fn drop(&mut self) {
                    SCANNING.store(false, std::sync::atomic::Ordering::SeqCst);
                }
            }
            let _release = Release;
            scan_site(&target_copy, &wanted_profile, profiles)
        });
        // Up to 24 profiles may each wait eight seconds for their OS key store.
        // Leave room for snapshots on a busy disk instead of discarding a valid full scan.
        let (checked, unavailable, sessions) =
            tokio::time::timeout(std::time::Duration::from_secs(300), scan)
                .await
                .map_err(|_| crate::apperr::AppError::api("browser_unavailable", 408))?
                .map_err(|_| crate::apperr::AppError::api("browser_unavailable", 422))?;
        for session in sessions {
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
                let mut credential = self.credential(&id)?;
                let changed = serde_json::to_value(&credential.cookies)?
                    != serde_json::to_value(&session.cookies)?
                    || credential.local_storage != session.storage;
                credential.cookies = session.cookies;
                credential.local_storage = session.storage;
                if changed {
                    credential.headers.clear();
                }
                self.vault
                    .lock()
                    .map_err(|_| crate::apperr::AppError::api("internal", 500))?
                    .set("accounts", &id, &serde_json::to_string(&credential)?)?;
                self.state.update(|d| {
                    if let Some(account) = d.accounts.get_mut(&id) {
                        account.cookie_count = credential.cookies.len();
                        account.status = if credential.cookies.is_empty()
                            && credential.local_storage.is_empty()
                        {
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
                    d.sites.entry(effective_id.clone()).or_default().account_id = before;
                    Ok(())
                })?;
                let mut credential = self.credential(&account.id)?;
                credential.source_profile = session.profile.id;
                self.vault
                    .lock()
                    .map_err(|_| crate::apperr::AppError::api("internal", 500))?
                    .set(
                        "accounts",
                        &account.id,
                        &serde_json::to_string(&credential)?,
                    )?;
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
                let mut credential = self.credential(&id)?;
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
                        if let Some(identity) = capture.identity {
                            credential.headers = capture.headers;
                            credential.identity_source = capture.identity_url.clone();
                            self.vault
                                .lock()
                                .map_err(|_| crate::apperr::AppError::api("internal", 500))?
                                .set("accounts", &id, &serde_json::to_string(&credential)?)?;
                            self.state.update(|data| {
                                if let Some(view) = data.accounts.get_mut(&id) {
                                    crate::accounts::apply_identity(view, identity);
                                    view.checked_at = util::now();
                                }
                                let meta = data.sites.entry(effective_id.clone()).or_default();
                                if !meta.identity_paths.contains(&capture.identity_url) {
                                    meta.identity_paths.push(capture.identity_url);
                                }
                                for observation in capture.observations {
                                    if !meta.observations.iter().any(|old| {
                                        old.method == observation.method
                                            && old.path == observation.path
                                    }) {
                                        meta.observations.push(observation);
                                    }
                                }
                                Ok(())
                            })?;
                            // Replay the captured read through the executor before selecting it.
                            let _ = self.refresh_account(&id).await;
                        }
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
        let origin = crate::runtime::origin(url)?;
        let data = self.state.read()?;
        let usable = |id: &str| {
            data.accounts.get(id).is_some_and(|account| {
                account.site_id == site_id
                    && account.site_url == origin
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
            state.sites.entry(site_id.into()).or_default().account_id = id.clone();
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
        );
        assert_eq!(checked, 0);
        assert_eq!(unavailable, 0);
        assert!(found.is_empty());
    }
}

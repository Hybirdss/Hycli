//! Separate persistent browser authority, process control, and one-use opening tickets.
use super::{
    Web,
    private_file::{PersistentFile, PrivateFile},
    token,
};
use crate::{
    apperr::{AppError, AppResult},
    runtime::Runtime,
    util,
};
use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderMap, Method, header},
    middleware::Next,
    response::{Html, IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const TICKET_TTL: Duration = Duration::from_secs(120);
const MAX_TICKETS: usize = 32;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowserSession {
    version: u8,
    session: String,
    csrf: String,
}
struct ControlCredential(String);
struct PendingBootstrap {
    expires: Instant,
    // Owned here: opening a browser is not evidence that it consumed the ticket.
    _artifact: Option<PrivateFile>,
}
#[derive(Default)]
struct Tickets {
    pending: BTreeMap<String, PendingBootstrap>,
    closed: bool,
}
pub(super) struct DashboardAuth {
    browser: BrowserSession,
    control: ControlCredential,
    control_file: PrivateFile,
    tickets: Mutex<Tickets>,
}

fn valid_secret(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn handoff_html(port: u16, instance_id: &str, ticket: &str) -> String {
    format!(
        r#"<!doctype html><meta charset="utf-8"><meta name="referrer" content="no-referrer"><meta name="hycli-instance" content="{instance_id}"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; base-uri 'none'; form-action 'none'"><meta http-equiv="refresh" content="0;url=http://127.0.0.1:{port}/bootstrap#ticket={ticket}"><title>Opening Hycli</title>Opening Hycli…"#
    )
}
pub(super) fn validate_opening(path: &Path, port: u16, instance_id: &str) -> std::io::Result<()> {
    let bytes = super::private_file::read_private(path)?;
    let value = std::str::from_utf8(&bytes)
        .map_err(|_| std::io::Error::other("Invalid dashboard opening"))?;
    let ticket = value
        .split_once("/bootstrap#ticket=")
        .and_then(|(_, tail)| tail.split_once('"').map(|(ticket, _)| ticket));
    if !ticket.is_some_and(|ticket| {
        valid_secret(ticket) && value == handoff_html(port, instance_id, ticket)
    }) {
        return Err(std::io::Error::other("Invalid dashboard opening"));
    }
    Ok(())
}
impl DashboardAuth {
    fn new(root: &Path, port: u16, instance_id: &str) -> AppResult<Self> {
        // Never accept the legacy launch-derived session. The versioned private
        // store is created once, so new browser cookies survive later restarts.
        let initial = BrowserSession {
            version: 2,
            session: token(),
            csrf: token(),
        };
        let store =
            PersistentFile::open_or_create(root, "sessions.json", &serde_json::to_vec(&initial)?)?;
        let browser: BrowserSession = serde_json::from_slice(&store.read()?)?;
        if browser.version != 2 || !valid_secret(&browser.session) || !valid_secret(&browser.csrf) {
            return Err(AppError::api("invalid_dashboard_session", 500));
        }
        let legacy = root.join("dashboard-session.json");
        match std::fs::remove_file(legacy) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let control = ControlCredential(token());
        let control_file = PrivateFile::create(&serde_json::to_vec(
            &json!({"version":1,"port":port,"instance_id":instance_id,"credential":control.0,"root":std::fs::canonicalize(root)?}),
        )?)?;
        Ok(Self {
            browser,
            control,
            control_file,
            tickets: Mutex::new(Tickets::default()),
        })
    }
    fn prune(tickets: &mut Tickets, now: Instant) {
        tickets.pending.retain(|_, pending| now < pending.expires);
    }
    pub(super) fn expire(&self) {
        if let Ok(mut tickets) = self.tickets.lock() {
            Self::prune(&mut tickets, Instant::now());
        }
    }
    pub(super) fn revoke_all(&self) {
        if let Ok(mut tickets) = self.tickets.lock() {
            tickets.closed = true;
            tickets.pending.clear();
        }
    }
    fn issue(
        &self,
        port: u16,
        instance_id: &str,
        artifact: bool,
    ) -> AppResult<(String, Option<PathBuf>)> {
        let mut tickets = self
            .tickets
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        let now = Instant::now();
        Self::prune(&mut tickets, now);
        if tickets.closed {
            return Err(AppError::api("shutting_down", 503));
        }
        if tickets.pending.len() >= MAX_TICKETS {
            return Err(AppError::api("too_many_openings", 429));
        }
        let ticket = token();
        let file = if artifact {
            // This file is the only transport that contains the ticket. Its path
            // and the public dashboard URL carry no authentication authority.
            let html = handoff_html(port, instance_id, &ticket);
            Some(PrivateFile::create(html.as_bytes())?)
        } else {
            None
        };
        let path = file.as_ref().map(|f| f.path().to_path_buf());
        tickets.pending.insert(
            util::hash(ticket.as_bytes()),
            PendingBootstrap {
                expires: now + TICKET_TTL,
                _artifact: file,
            },
        );
        Ok((ticket, path))
    }
    fn consume(&self, ticket: &str) -> bool {
        if !valid_secret(ticket) {
            return false;
        }
        let Ok(mut tickets) = self.tickets.lock() else {
            return false;
        };
        let now = Instant::now();
        Self::prune(&mut tickets, now);
        if tickets.closed {
            return false;
        }
        // Remove under one lock before responding. File cleanup failure can never
        // restore the removed entry or make a second request succeed.
        tickets
            .pending
            .remove(&util::hash(ticket.as_bytes()))
            .is_some()
    }
}
impl Web {
    pub fn new(core: Arc<Runtime>, port: u16) -> AppResult<Self> {
        let instance_id = util::id();
        let auth = Arc::new(DashboardAuth::new(&core.root, port, &instance_id)?);
        Ok(Self {
            core,
            port,
            auth,
            instance_id,
            shutdown: tokio::sync::watch::channel(false).0,
            pairs: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }
    pub fn prepare_opening(&self) -> AppResult<PathBuf> {
        if *self.shutdown.borrow() {
            return Err(AppError::api("shutting_down", 503));
        }
        self.auth
            .issue(self.port, &self.instance_id, true)?
            .1
            .ok_or_else(|| AppError::api("internal", 500))
    }
    pub(super) fn control_path(&self) -> PathBuf {
        self.auth.control_file.path().to_path_buf()
    }
    #[cfg(any(test, feature = "dashboard-test-support"))]
    pub fn test_bootstrap_ticket(&self) -> String {
        self.auth
            .issue(self.port, &self.instance_id, false)
            .unwrap()
            .0
    }
}
fn session_cookie(headers: &HeaderMap) -> &str {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .find_map(|p| p.trim().strip_prefix("hycli_session="))
        .unwrap_or("")
}
fn control_authorized(web: &Web, headers: &HeaderMap) -> bool {
    let credential = headers
        .get("x-hycli-control")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let instance = headers
        .get("x-hycli-instance")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    !headers.contains_key(header::ORIGIN)
        && !headers.contains_key("sec-fetch-site")
        && util::constant_eq(credential, &web.auth.control.0)
        && util::constant_eq(instance, &web.instance_id)
}
pub(super) async fn guard(State(web): State<Web>, request: Request, next: Next) -> Response {
    let fail = || AppError::api("forbidden", 403).into_response();
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if host != format!("127.0.0.1:{}", web.port) && host != format!("localhost:{}", web.port) {
        return fail();
    }
    let path = request.uri().path();
    let control = matches!(
        path,
        "/control/probe" | "/control/open" | "/control/shutdown"
    );
    let landing = path == "/bootstrap"
        && matches!(*request.method(), Method::GET | Method::HEAD)
        && request.uri().query().is_none();
    if control {
        if request.method() != Method::POST || !control_authorized(&web, request.headers()) {
            return fail();
        }
    } else if !path.starts_with("/bridge/") {
        // The one static landing document accepts navigation from the private
        // file. It contains no secret and grants no API or SPA exception.
        if !landing {
            if let Some(origin) = request.headers().get(header::ORIGIN) {
                if origin.to_str().ok() != Some(format!("http://{host}").as_str()) {
                    return fail();
                }
            }
            if request
                .headers()
                .get("sec-fetch-site")
                .is_some_and(|v| v != "same-origin" && v != "none")
            {
                return fail();
            }
        }
        if path.starts_with("/api/") && !matches!(path, "/api/session" | "/api/bootstrap") {
            if !util::constant_eq(session_cookie(request.headers()), &web.auth.browser.session) {
                return AppError::api("session_expired", 401).into_response();
            }
            if !matches!(*request.method(), Method::GET | Method::HEAD) {
                let csrf = request
                    .headers()
                    .get("x-hycli-csrf")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                if !util::constant_eq(csrf, &web.auth.browser.csrf) {
                    return AppError::api("session_expired", 401).into_response();
                }
            }
        }
    }
    if *web.shutdown.borrow() {
        web.auth.revoke_all();
        return AppError::api("shutting_down", 503).into_response();
    }
    web.auth.expire();
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("referrer-policy", "no-referrer".parse().unwrap());
    if !headers.contains_key("content-security-policy") {
        headers.insert("content-security-policy", "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; font-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'".parse().unwrap());
    }
    response
}
fn session_response(web: &Web) -> Response {
    (
        [(
            header::SET_COOKIE,
            format!(
                "hycli_session={}; Path=/; HttpOnly; SameSite=Strict",
                web.auth.browser.session
            ),
        )],
        Json(json!({"csrf": web.auth.browser.csrf, "instance_id": web.instance_id})),
    )
        .into_response()
}
pub(super) async fn session(State(web): State<Web>, headers: HeaderMap) -> Response {
    if !util::constant_eq(session_cookie(&headers), &web.auth.browser.session) {
        return AppError::api("launch_required", 401).into_response();
    }
    session_response(&web)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Bootstrap {
    ticket: String,
}
pub(super) async fn bootstrap(State(web): State<Web>, Json(body): Json<Bootstrap>) -> Response {
    if !web.auth.consume(&body.ticket) {
        return AppError::api("launch_required", 401).into_response();
    }
    session_response(&web)
}
pub(super) async fn control_probe(State(web): State<Web>) -> Json<serde_json::Value> {
    Json(json!({"instance_id": web.instance_id}))
}
pub(super) async fn control_open(State(web): State<Web>) -> AppResult<Json<serde_json::Value>> {
    Ok(Json(json!({"path": web.prepare_opening()?})))
}
pub(super) async fn landing() -> Html<&'static str> {
    Html(
        "<!doctype html><meta charset=\"utf-8\"><meta name=\"referrer\" content=\"no-referrer\"><title>Hycli</title><p id=\"status\">Opening Hycli…</p><script src=\"/bootstrap.js\"></script>",
    )
}
pub(super) async fn landing_script() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("bootstrap.js"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        auth: Arc<DashboardAuth>,
        root: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("hycli-auth-unit-{}", util::id()));
            std::fs::create_dir(&root).unwrap();
            let auth = Arc::new(DashboardAuth::new(&root, 4318, "synthetic-instance").unwrap());
            Self { auth, root }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    #[test]
    fn independent_openings_are_consumed_exactly_once() {
        let fixture = Fixture::new();
        let (first, _) = fixture
            .auth
            .issue(4318, "synthetic-instance", false)
            .unwrap();
        let (second, _) = fixture
            .auth
            .issue(4318, "synthetic-instance", false)
            .unwrap();
        assert!(fixture.auth.consume(&first));
        assert!(!fixture.auth.consume(&first));
        assert!(fixture.auth.consume(&second));
        assert!(!fixture.auth.consume(&second));
    }
    #[test]
    fn simultaneous_consumers_have_one_winner() {
        let fixture = Fixture::new();
        let (ticket, _) = fixture
            .auth
            .issue(4318, "synthetic-instance", false)
            .unwrap();
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let auth = fixture.auth.clone();
                let ticket = ticket.clone();
                std::thread::spawn(move || usize::from(auth.consume(&ticket)))
            })
            .collect();
        assert_eq!(
            threads
                .into_iter()
                .map(|t| t.join().unwrap())
                .sum::<usize>(),
            1
        );
    }
    #[test]
    fn expiry_and_shutdown_are_irreversible_and_bounded() {
        let fixture = Fixture::new();
        let (ticket, _) = fixture
            .auth
            .issue(4318, "synthetic-instance", false)
            .unwrap();
        fixture
            .auth
            .tickets
            .lock()
            .unwrap()
            .pending
            .get_mut(&util::hash(ticket.as_bytes()))
            .unwrap()
            .expires = Instant::now() - Duration::from_secs(1);
        assert!(!fixture.auth.consume(&ticket));
        for _ in 0..MAX_TICKETS {
            fixture
                .auth
                .issue(4318, "synthetic-instance", false)
                .unwrap();
        }
        assert!(
            fixture
                .auth
                .issue(4318, "synthetic-instance", false)
                .is_err()
        );
        fixture.auth.revoke_all();
        assert!(
            fixture
                .auth
                .issue(4318, "synthetic-instance", false)
                .is_err()
        );
        assert!(fixture.auth.tickets.lock().unwrap().pending.is_empty());
    }
    #[test]
    fn private_artifact_is_owned_until_consumption_or_shutdown() {
        let fixture = Fixture::new();
        let (ticket, path) = fixture
            .auth
            .issue(4318, "synthetic-instance", true)
            .unwrap();
        let path = path.unwrap();
        assert!(path.exists());
        assert!(!path.to_string_lossy().contains(&ticket));
        assert!(fixture.auth.consume(&ticket));
        assert!(!path.exists());
        let (_, path) = fixture
            .auth
            .issue(4318, "synthetic-instance", true)
            .unwrap();
        let path = path.unwrap();
        fixture.auth.revoke_all();
        assert!(!path.exists());
    }
    #[test]
    fn browser_reconnects_but_legacy_and_process_authority_do_not() {
        let fixture = Fixture::new();
        let legacy = fixture.root.join("dashboard-session.json");
        std::fs::write(&legacy, br#"{"session":"dummy-old-session","csrf":"dummy-old-csrf","launch":"dummy-old-launch"}"#).unwrap();
        let restarted = DashboardAuth::new(&fixture.root, 4318, "new-instance").unwrap();
        assert_eq!(fixture.auth.browser.session, restarted.browser.session);
        assert_eq!(fixture.auth.browser.csrf, restarted.browser.csrf);
        assert_ne!(fixture.auth.control.0, restarted.control.0);
        assert!(!legacy.exists());
        let (ticket, _) = fixture
            .auth
            .issue(4318, "synthetic-instance", false)
            .unwrap();
        assert!(!restarted.consume(&ticket));
    }
    #[test]
    fn failed_artifact_deletion_never_restores_a_consumed_ticket() {
        let fixture = Fixture::new();
        let (ticket, path) = fixture
            .auth
            .issue(4318, "synthetic-instance", true)
            .unwrap();
        let path = path.unwrap();
        // Synthetic fault: make unlink fail without changing any permissions.
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(fixture.auth.consume(&ticket));
        assert!(!fixture.auth.consume(&ticket));
        assert!(fixture.auth.tickets.lock().unwrap().pending.is_empty());
        std::fs::remove_dir(&path).unwrap();
        std::fs::remove_dir(path.parent().unwrap()).unwrap();
    }
    #[test]
    fn prepared_artifact_validation_requires_matching_type_and_instance() {
        let fixture = Fixture::new();
        let (_, path) = fixture
            .auth
            .issue(4318, "synthetic-instance", true)
            .unwrap();
        let path = path.unwrap();
        assert!(validate_opening(&path, 4318, "synthetic-instance").is_ok());
        assert!(validate_opening(&path, 4319, "synthetic-instance").is_err());
        assert!(validate_opening(&path, 4318, "other-instance").is_err());
        assert!(
            validate_opening(fixture.auth.control_file.path(), 4318, "synthetic-instance").is_err()
        );
        assert!(
            validate_opening(
                Path::new("not-a-private-opening"),
                4318,
                "synthetic-instance"
            )
            .is_err()
        );
    }
    #[tokio::test]
    async fn control_and_browser_authority_remain_separate() {
        let fixture = Fixture::new();
        let core = Runtime::open(&fixture.root, None, None, false, true, false).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let web = Web::new(core, port).unwrap();
        let control = web.auth.control.0.clone();
        let instance = web.instance_id.clone();
        let ticket = web.test_bootstrap_ticket();
        let server = tokio::spawn(async move {
            axum::serve(listener, super::super::router(web))
                .await
                .unwrap();
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let base = format!("http://127.0.0.1:{port}");
        // Only the exact static landing document has a navigation exception.
        for (path, status) in [
            ("/bootstrap", 200),
            ("/bootstrap?extra=1", 403),
            ("/bootstrap/", 403),
            ("/", 403),
            ("/api/session", 403),
        ] {
            let response = client
                .get(format!("{base}{path}"))
                .header("sec-fetch-site", "cross-site")
                .send()
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), status);
        }
        let response = client
            .post(format!("{base}/api/bootstrap"))
            .header("origin", "null")
            .json(&json!({"ticket":ticket}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 403);
        let probe = client
            .post(format!("{base}/control/probe"))
            .header("x-hycli-control", &control)
            .header("x-hycli-instance", &instance)
            .send()
            .await
            .unwrap();
        assert_eq!(probe.status(), 200);
        assert!(probe.headers().get(header::SET_COOKIE).is_none());
        let response = client
            .get(format!("{base}/api/state"))
            .header("x-hycli-control", &control)
            .header("x-hycli-instance", &instance)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
        let session = client
            .post(format!("{base}/api/bootstrap"))
            .json(&json!({"ticket":ticket}))
            .send()
            .await
            .unwrap();
        assert_eq!(session.status(), 200);
        let cookie = session
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let response = client
            .post(format!("{base}/control/probe"))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 403);
        let response = client
            .get(format!("{base}/api/session"))
            .header(header::COOKIE, &cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        server.abort();
        let _ = server.await;
    }
}

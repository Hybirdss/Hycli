//! Browser-to-executor sign-in transfer. Models receive only AccountView.
use crate::{
    apperr::{AppError, AppResult},
    net, policy, util,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct AccountView {
    pub id: String,
    pub site_id: String,
    pub site_url: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub browser: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub profile: String,
    pub status: String,
    pub verified_identity: bool,
    pub checked_at: String,
    pub cookie_count: usize,
    pub created_at: String,
}
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct PartitionKey {
    #[serde(default, rename = "topLevelSite")]
    pub top_level_site: String,
    #[serde(default, rename = "hasCrossSiteAncestor")]
    pub has_cross_site_ancestor: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    #[serde(default = "cookie_path")]
    pub path: String,
    #[serde(default)]
    pub secure: bool,
    #[serde(default, rename = "httpOnly", alias = "http_only")]
    pub http_only: bool,
    #[serde(default, rename = "hostOnly", alias = "host_only")]
    pub host_only: bool,
    #[serde(
        default,
        rename = "expirationDate",
        alias = "expires",
        alias = "expiry"
    )]
    pub expires: Option<f64>,
    #[serde(default, rename = "sameSite")]
    pub same_site: String,
    #[serde(default, rename = "storeId")]
    pub store_id: String,
    #[serde(default, rename = "partitionKey")]
    pub partition_key: Option<PartitionKey>,
}
fn cookie_path() -> String {
    "/".into()
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Credential {
    pub origin: String,
    pub cookies: Vec<Cookie>,
    #[serde(default)]
    pub identity_source: String,
    #[serde(default)]
    pub browser_session: String,
    #[serde(default)]
    pub extension_origin: String,
    #[serde(default)]
    pub browser_session_expires_ms: i64,
    #[serde(default)]
    pub source_profile: String,
    #[serde(default)]
    pub local_storage: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub recipe: Option<crate::session_recipe::ConnectionRecipe>,
}
impl Credential {
    pub fn cookie_header(&self, raw_url: &str) -> AppResult<String> {
        let url = net::validate_url(raw_url)?;
        if url.origin().ascii_serialization() != self.origin {
            return Err(AppError::api("unsafe_target", 400));
        }
        let mut cookies: Vec<_> = self
            .cookies
            .iter()
            .filter(|c| cookie_matches(c, &url))
            .collect();
        cookies.sort_by(|a, b| b.path.len().cmp(&a.path.len()));
        Ok(cookies
            .iter()
            .map(|c| format!("{}={}", c.name, c.value))
            .collect::<Vec<_>>()
            .join("; "))
    }
    pub fn redaction_values(&self) -> Vec<String> {
        let mut values: Vec<_> = self
            .cookies
            .iter()
            .map(|c| c.value.clone())
            .chain(self.headers.values().cloned())
            .filter(|v| !v.is_empty())
            .collect();
        fn leaves(value: &Value, sensitive: bool, output: &mut Vec<String>) {
            match value {
                Value::String(s) if sensitive && !s.is_empty() => output.push(s.clone()),
                Value::Array(items) => items.iter().for_each(|v| leaves(v, sensitive, output)),
                Value::Object(items) => items
                    .iter()
                    .for_each(|(key, v)| leaves(v, sensitive || util::secret_field(key), output)),
                _ => {}
            }
        }
        for (key, raw) in &self.local_storage {
            let sensitive = util::secret_field(key);
            if let Ok(value) = serde_json::from_str::<Value>(raw) {
                leaves(&value, sensitive, &mut values);
            } else if sensitive {
                values.push(raw.clone());
            }
        }
        if let Some(recipe) = &self.recipe {
            if let Ok(headers) = recipe.resolve_headers(self) {
                for rule in &recipe.headers {
                    if let Some(value) = headers.get(&rule.header.to_ascii_lowercase()) {
                        values.push(value.clone());
                        if let Some(raw) = value.strip_prefix(&rule.prefix) {
                            values.push(raw.into());
                        }
                    }
                }
            }
        }
        for header in self.headers.values() {
            if let Some((_, token)) = header.split_once(' ') {
                if !token.is_empty() {
                    values.push(token.into());
                }
            }
        }
        values.sort_by_key(|s| std::cmp::Reverse(s.len()));
        values.dedup();
        values
    }
    pub fn request_headers(&self, url: &str) -> AppResult<Vec<(String, String)>> {
        let cookie = self.cookie_header(url)?;
        let mut headers: Vec<_> = self
            .headers
            .iter()
            .filter(|(k, _)| crate::session_recipe::allowed_header(k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if !cookie.is_empty() {
            headers.push(("cookie".into(), cookie));
        }
        Ok(headers)
    }
}
pub fn browser_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization" | "x-csrf-token" | "x-xsrf-token" | "x-csrftoken" | "x-api-key" | "api-key"
    )
}
pub fn cookie_matches(cookie: &Cookie, url: &url::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let domain = cookie.domain.trim_start_matches('.').to_ascii_lowercase();
    if host != domain && (cookie.host_only || !host.ends_with(&format!(".{domain}"))) {
        return false;
    }
    if cookie.secure && url.scheme() != "https" {
        return false;
    }
    if cookie
        .expires
        .is_some_and(|expires| expires > 0.0 && expires <= chrono::Utc::now().timestamp() as f64)
    {
        return false;
    }
    if !url.path().starts_with(&cookie.path)
        || !(cookie.path.ends_with('/')
            || cookie.path == url.path()
            || url.path().as_bytes().get(cookie.path.len()) == Some(&b'/'))
    {
        return false;
    }
    if let Some(partition) = &cookie.partition_key {
        let Ok(top) = net::validate_url(&partition.top_level_site) else {
            return false;
        };
        let Some(top_host) = top.host_str() else {
            return false;
        };
        if partition.has_cross_site_ancestor
            || top.scheme() != url.scheme()
            || !(host == top_host || host.ends_with(&format!(".{top_host}")))
        {
            return false;
        }
    }
    true
}
pub fn parse_cookies(content: &str, site_url: &str) -> AppResult<Vec<Cookie>> {
    if content.len() > 1024 * 1024 {
        return Err(AppError::api("too_large", 413));
    }
    let url = net::validate_url(site_url)?;
    let cookies: Vec<Cookie> = if content.trim_start().starts_with(['[', '{']) {
        let value: Value =
            serde_json::from_str(content).map_err(|_| AppError::api("data_invalid", 400))?;
        serde_json::from_value(value.get("cookies").cloned().unwrap_or(value))
            .map_err(|_| AppError::api("data_invalid", 400))?
    } else {
        let mut cookies = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || (line.starts_with('#') && !line.starts_with("#HttpOnly_")) {
                continue;
            }
            let http_only = line.starts_with("#HttpOnly_");
            let line = line.trim_start_matches("#HttpOnly_");
            let parts: Vec<_> = line.splitn(7, '\t').collect();
            if parts.len() != 7 {
                return Err(AppError::api("data_invalid", 400));
            }
            cookies.push(Cookie {
                name: parts[5].into(),
                value: parts[6].into(),
                domain: parts[0].into(),
                path: parts[2].into(),
                secure: parts[3].eq_ignore_ascii_case("TRUE"),
                http_only,
                host_only: !parts[1].eq_ignore_ascii_case("TRUE"),
                expires: parts[4].parse().ok(),
                same_site: String::new(),
                store_id: String::new(),
                partition_key: None,
            });
        }
        cookies
    };
    normalize(cookies, &url)
}
pub fn normalize(cookies: Vec<Cookie>, url: &url::Url) -> AppResult<Vec<Cookie>> {
    if cookies.len() > 2000 {
        return Err(AppError::api("too_large", 413));
    }
    let host = url
        .host_str()
        .ok_or_else(|| AppError::api("bad_url", 400))?;
    let mut valid = Vec::new();
    for mut cookie in cookies {
        let domain = cookie.domain.trim_start_matches('.').to_ascii_lowercase();
        if domain.is_empty()
            || !(host == domain
                || (!cookie.host_only
                    && domain.contains('.')
                    && host.ends_with(&format!(".{domain}"))))
        {
            continue;
        }
        if cookie.name.is_empty()
            || cookie.name.len() > 256
            || cookie.value.len() > 16384
            || cookie
                .name
                .bytes()
                .any(|b| b <= 0x20 || b >= 0x7f || b"()<>@,;:\\\"/[]?={}".contains(&b))
            || cookie
                .value
                .bytes()
                .any(|b| b < 0x20 || b == 0x7f || b == b';' || b == b'\r' || b == b'\n')
        {
            return Err(AppError::api("data_invalid", 400));
        }
        if !cookie.path.starts_with('/') || cookie.path.contains(['\r', '\n']) {
            return Err(AppError::api("data_invalid", 400));
        }
        if cookie.expires.is_some_and(|n| !n.is_finite()) {
            return Err(AppError::api("data_invalid", 400));
        }
        if cookie
            .expires
            .is_some_and(|n| n > 0.0 && n <= chrono::Utc::now().timestamp() as f64)
        {
            continue;
        }
        if let Some(partition) = &cookie.partition_key {
            let Ok(top) = net::validate_url(&partition.top_level_site) else {
                continue;
            };
            if partition.has_cross_site_ancestor
                || top.scheme() != url.scheme()
                || !top
                    .host_str()
                    .is_some_and(|h| host == h || host.ends_with(&format!(".{h}")))
            {
                continue;
            }
        }
        cookie.domain = domain;
        valid.push(cookie);
    }
    if valid.is_empty() {
        return Err(AppError::api("auth_required", 401));
    }
    Ok(valid)
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Identity {
    pub email: String,
    pub user_id: String,
    pub name: String,
}
pub fn identity_endpoint(url: &url::Url) -> bool {
    let path = url.path().to_ascii_lowercase();
    url.query().is_none()
        && !policy::dangerous_intent(&path)
        && path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .is_some_and(|segment| {
                [
                    "me",
                    "@me",
                    "viewer",
                    "userinfo",
                    "whoami",
                    "profile",
                    "account",
                    "session",
                    "current-user",
                    "current_user",
                ]
                .contains(&segment)
            })
}
pub fn extract_identity(value: &Value) -> Option<Identity> {
    // Only known current-account containers; never infer identity from lists of people.
    let paths = [
        "/user",
        "/account",
        "/profile",
        "/me",
        "/viewer",
        "/currentUser",
        "/current_user",
        "/data/me",
        "/data/viewer",
        "/data/currentUser",
        "/data/current_user",
        "/data/user",
        "/data/account",
        "/session/user",
        "/data",
        "",
    ];
    for path in paths {
        let Some(object) = value.pointer(path).and_then(Value::as_object) else {
            continue;
        };
        let text = |keys: &[&str]| {
            keys.iter()
                .find_map(|key| {
                    object.get(*key).and_then(|v| {
                        if v.is_string() || v.is_number() {
                            Some(util::plain(v))
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_default()
        };
        let email = text(&["email", "emailAddress", "email_address"]);
        let user_id = text(&["id", "userId", "user_id", "username", "login", "sub"]);
        let name = text(&["name", "displayName", "display_name", "full_name"]);
        let identifiable_root = path != ""
            || !email.is_empty()
            || !name.is_empty()
            || object.contains_key("username")
            || object.contains_key("login")
            || object.contains_key("sub")
            || object.contains_key("userId")
            || object.contains_key("user_id");
        if !identifiable_root {
            continue;
        }
        if (!email.is_empty() && email.contains('@') && email.len() < 255)
            || (!user_id.is_empty() && user_id.len() < 255)
        {
            return Some(Identity {
                email,
                user_id,
                name: name.chars().take(200).collect(),
            });
        }
    }
    None
}
pub fn apply_identity(view: &mut AccountView, identity: Identity) {
    view.email = identity.email;
    view.user_id = identity.user_id;
    view.name = identity.name;
    view.label = if !view.email.is_empty() {
        view.email.clone()
    } else if !view.user_id.is_empty() {
        view.user_id.clone()
    } else {
        view.name.clone()
    };
    view.verified_identity = true;
    view.status = "connected".into();
    view.checked_at = util::now();
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Observation {
    pub method: String,
    pub path: String,
    pub query: std::collections::BTreeMap<String, String>,
    pub body: std::collections::BTreeMap<String, String>,
    pub status: u16,
    pub observed_at: String,
}

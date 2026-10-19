use crate::{
    apperr::{AppError, AppResult},
    net::Response,
    policy,
    runtime::Runtime,
};
use std::collections::BTreeSet;
use url::Url;

pub fn documentation_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    [
        "/docs",
        "/reference",
        "/resources",
        "/developers",
        "/documentation",
        "/openapi",
        "swagger",
        "llms.txt",
        "llms-full.txt",
        "docs.",
        "developer.",
        "api-reference",
        ".md",
        ".yaml",
        ".yml",
        ".json",
    ]
    .iter()
    .any(|part| lower.contains(part))
}

pub fn priority(url: &str) -> i32 {
    let lower = url.to_ascii_lowercase();
    if [
        ".mp4", ".webm", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".woff", ".woff2", ".ttf",
        ".css", ".ico", ".mp3", ".zip", ".exe", ".dmg",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
    {
        return -100;
    }
    if lower.contains("llms.txt") {
        return 1000;
    }
    if lower.contains("llms-full.txt") {
        return 50;
    }
    if lower.contains("openapi") || lower.contains("swagger") {
        return 950;
    }
    if lower.ends_with("/reference.md") || lower.ends_with("/reference") {
        return 920;
    }
    if lower.contains("/docs/") || lower.ends_with("/docs") {
        return 850;
    }
    if lower.contains("docs.") || lower.contains("developer.") {
        return 830;
    }
    if lower.contains("reference") && !lower.contains("/resources/") {
        return 800;
    }
    if lower.contains("oauth")
        || lower.contains("authentication")
        || lower.contains("authorization")
    {
        return 780;
    }
    if lower.contains("/resources/") || lower.contains("api-reference/") {
        return 750;
    }
    if lower.ends_with(".md") {
        return 700;
    }
    if lower.ends_with(".js") {
        if ["polyfill", "vendor", "framework", "runtime", "turbopack"]
            .iter()
            .any(|noise| lower.contains(noise))
        {
            return 100;
        }
        return 300;
    }
    200
}

pub fn api_prefixes(initial: &Url, text: &str) -> BTreeSet<String> {
    let mut prefixes: BTreeSet<String> = regex::Regex::new(r#"https?://[^\s<>"'`\\)\]}]+"#)
        .expect("constant")
        .find_iter(text)
        .filter_map(|m| Url::parse(m.as_str()).ok())
        .filter(|url| url.origin() == initial.origin() && url.query().is_none())
        .map(|url| url.path().trim_end_matches('/').to_string())
        .filter(|path| {
            regex::Regex::new(r"^/(?:api(?:/v[0-9]+)?|v[0-9]+)$")
                .unwrap()
                .is_match(path)
        })
        .collect();
    // Some references publish /api/v{version} plus an explicit version availability table.
    if text.contains(&format!(
        "{}/api/v{{",
        initial.origin().ascii_serialization()
    )) {
        for row in regex::Regex::new(r"(?i)\b([0-9]{1,4})\s*(?:\|\s*)?(?:Available|Supported)\b")
            .unwrap()
            .captures_iter(text)
        {
            prefixes.insert(format!("/api/v{}", &row[1]));
        }
    }
    prefixes
}

pub fn route_matches(path: &str, documented: &str, prefixes: &BTreeSet<String>) -> bool {
    fn shape(path: &str) -> String {
        regex::Regex::new(r"\{\{[^}]+\}\}|\{[^}]+\}")
            .unwrap()
            .replace_all(path, "{}")
            .into_owned()
    }
    fn matches(path: &str, template: &str) -> bool {
        if shape(path) == shape(template) {
            return true;
        }
        let variables = regex::Regex::new(r"\{\{[^}/]+\}\}|\{[^}/]+\}").unwrap();
        let mut pattern = String::from("^");
        let mut end = 0;
        for variable in variables.find_iter(template) {
            pattern.push_str(&regex::escape(&template[end..variable.start()]));
            pattern.push_str("[^/]+");
            end = variable.end();
        }
        pattern.push_str(&regex::escape(&template[end..]));
        pattern.push('$');
        regex::Regex::new(&pattern).is_ok_and(|expression| expression.is_match(path))
    }
    matches(path, documented)
        || prefixes
            .iter()
            .any(|prefix| matches(path, &format!("{prefix}{documented}")))
}

pub fn ranked<'a>(links: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    let mut result: Vec<_> = links.into_iter().cloned().collect();
    result.sort_by_key(|url| {
        (
            std::cmp::Reverse(priority(url) + if url.ends_with(".md") { 30 } else { 0 }),
            url.clone(),
        )
    });
    result
}

pub fn published_links(base: &Url, text: &str) -> BTreeSet<String> {
    let mut links = BTreeSet::new();
    let expressions = [
        r#"(?i)(?:href|src)\s*=\s*["']([^"'<>\s]+)["']"#,
        r#"\]\((https?://[^\s)]+|/[^\s)]+)\)"#,
        r#"["']((?:/|https?://)[^"'<>\s]{1,1000})["']"#,
    ];
    for expression in expressions {
        for capture in regex::Regex::new(expression)
            .expect("constant")
            .captures_iter(text)
            .take(4000)
        {
            let raw = capture[1].replace("&amp;", "&");
            if raw.contains(['{', '}', '(', ')', '*']) || raw.contains("/:") {
                continue;
            }
            let Ok(mut target) = base.join(&raw) else {
                continue;
            };
            target.set_fragment(None);
            if !matches!(target.scheme(), "http" | "https")
                || (base.scheme() == "https" && target.scheme() != "https")
                || !target.username().is_empty()
                || target.password().is_some()
                || target.query().is_some()
                || target.path().contains(['{', '}', '(', ')', '*'])
                || target.path().contains("/:")
                || !crate::browser_session::navigation_safe(&target)
            {
                continue;
            }
            // Link meaning is evaluated by the preparation agent, not a site-specific path list.
            links.insert(target.to_string());
        }
    }
    links
}

pub fn header_links(base: &Url, value: &str) -> BTreeSet<String> {
    regex::Regex::new(r"<([^>]+)>[^,]*")
        .expect("constant")
        .captures_iter(value)
        .filter(|capture| {
            [
                "llms-txt",
                "alternate",
                "service-desc",
                "describedby",
                "api-catalog",
            ]
            .iter()
            .any(|rel| capture[0].contains(rel))
        })
        .filter_map(|capture| base.join(&capture[1]).ok())
        .filter(|url| {
            matches!(url.scheme(), "https" | "http")
                && policy::safe_read_url(url)
                && url.query().is_none()
        })
        .map(|url| url.to_string())
        .collect()
}

/// HTTP routes written in reference prose/Markdown, rather than arbitrary navigation paths.
pub fn documented_routes(text: &str) -> BTreeSet<(String, String)> {
    let text = text
        .replace("\\/", "/")
        .replace("&lbrace;", "{")
        .replace("&rbrace;", "}");
    let parameter_links =
        regex::Regex::new(r"\[((?:\\)?\{[A-Za-z0-9_.-]+(?:\\)?\})\]\([^)]*\)").expect("constant");
    let text = parameter_links
        .replace_all(&text, "$1")
        .replace("\\{", "{")
        .replace("\\}", "}");
    let expression = regex::Regex::new(r"(?i)\b(GET|HEAD|POST|PUT|PATCH|DELETE)\s+(?:https?://[^/\s`<>]+)?(/[A-Za-z0-9_/@{}.:%+~!$&*=-]+)").expect("constant");
    expression
        .captures_iter(&text)
        .take(500)
        .filter_map(|capture| {
            let path = capture[2].trim_end_matches(['.', ':']).to_owned();
            if path.len() > 500 || path == "/" || path.contains("...") {
                return None;
            }
            Some((capture[1].to_ascii_uppercase(), path))
        })
        .collect()
}

impl Runtime {
    /// Public documentation can redirect to a published documentation host. Cookies and all
    /// other account material stay on the original origin; every hop still uses Network checks.
    pub(crate) async fn read_evidence_page(&self, raw: &str, account: &str) -> AppResult<Response> {
        let original = crate::net::validate_url(raw)?;
        let mut next = original.clone();
        for hop in 0..5 {
            if !policy::safe_read_url(&next)
                || (original.scheme() == "https" && next.scheme() != "https")
            {
                return Err(AppError::api("unsafe_target", 400));
            }
            let identity = if next.origin() == original.origin() {
                account
            } else {
                ""
            };
            let response = self.read_page(next.as_str(), identity).await?;
            if !matches!(response.status, 301 | 302 | 303 | 307 | 308) || hop == 4 {
                return Ok(response);
            }
            let location = response.header("location");
            if location.is_empty() {
                return Ok(response);
            }
            next = crate::net::validate_url(&response.url)?
                .join(&location)
                .map_err(|_| AppError::api("bad_url", 400))?;
            // Some sites still publish an HTTP location on their secure entry
            // point. Try its secure equivalent; never send a downgraded request.
            if original.scheme() == "https" && next.scheme() == "http" {
                next.set_scheme("https")
                    .map_err(|_| AppError::api("bad_url", 400))?;
            }
        }
        Err(AppError::api("request_failed", 502))
    }
}

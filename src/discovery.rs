//! Published documentation discovery. Links and redirects are evidence, never executable instructions.
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
        return 100;
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

/// Exact published server addresses; origin changes never imply a credential grant.
pub fn api_bases(base: &Url, text: &str) -> BTreeSet<String> {
    let mut servers = BTreeSet::new();
    if let Some(document) = api_schema(text) {
        if document.get("openapi").is_some() {
            let mut nodes = vec![&document];
            if let Some(paths) = document.get("paths").and_then(|v| v.as_object()) {
                for path in paths.values() {
                    nodes.push(path);
                    if let Some(operations) = path.as_object() {
                        nodes.extend(operations.values().filter(|v| v.is_object()));
                    }
                }
            }
            let mut root_has_server = false;
            for (index, node) in nodes.into_iter().enumerate() {
                for server in node
                    .get("servers")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                {
                    if let Some(raw) = server.get("url").and_then(|v| v.as_str()) {
                        let mut raw = raw.to_string();
                        if let Some(variables) = server.get("variables").and_then(|v| v.as_object())
                        {
                            for (name, variable) in variables {
                                if let Some(default) =
                                    variable.get("default").and_then(|v| v.as_str())
                                {
                                    raw = raw.replace(&format!("{{{name}}}"), default);
                                }
                            }
                        }
                        if !raw.contains(['{', '}']) {
                            if let Ok(url) = base.join(&raw) {
                                if safe_server(&url) {
                                    servers.insert(url.to_string().trim_end_matches('/').into());
                                    root_has_server |= index == 0;
                                }
                            }
                        }
                    }
                }
            }
            if !root_has_server
                && document
                    .get("servers")
                    .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty))
            {
                servers.insert(base.origin().ascii_serialization());
            }
        }
        if document.get("swagger").is_some() {
            let prefix = document
                .get("basePath")
                .and_then(|v| v.as_str())
                .unwrap_or("/");
            let schemes: Vec<_> = document
                .get("schemes")
                .and_then(|v| v.as_array())
                .map(|values| values.iter().filter_map(|v| v.as_str()).collect())
                .filter(|values: &Vec<&str>| !values.is_empty())
                .unwrap_or_else(|| vec![base.scheme()]);
            for scheme in schemes {
                let origin = document
                    .get("host")
                    .and_then(|v| v.as_str())
                    .map(|host| format!("{scheme}://{host}"))
                    .unwrap_or_else(|| {
                        let mut url = base.clone();
                        let _ = url.set_scheme(scheme);
                        url.origin().ascii_serialization()
                    });
                if let Ok(url) = Url::parse(&format!("{origin}{prefix}")) {
                    if safe_server(&url) {
                        servers.insert(url.to_string().trim_end_matches('/').into());
                    }
                }
            }
        }
    }
    let visible = crate::runtime::visible_text(text);
    let pattern = regex::Regex::new(r#"(?i)(?:base\s+url|api\s+(?:base\s+)?(?:url|endpoint|server))\s*[:=]?\s*(https?://[^\s<>"'`]+)"#).unwrap();
    for matched in pattern.captures_iter(&visible) {
        if let Ok(url) = Url::parse(matched[1].trim_end_matches(['.', ','])) {
            if url.query().is_none()
                && url.fragment().is_none()
                && url.username().is_empty()
                && url.password().is_none()
            {
                servers.insert(url.to_string().trim_end_matches('/').into());
            }
        }
    }
    // A published absolute request example also establishes its server origin,
    // even when the documentation does not use the literal words "Base URL".
    let routes = documented_routes(&visible);
    for matched in regex::Regex::new(r#"https?://[^\s<>"'`\\)\]]+"#)
        .unwrap()
        .find_iter(&visible)
    {
        if let Ok(url) = Url::parse(matched.as_str().trim_end_matches(['.', ','])) {
            if url.username().is_empty()
                && url.password().is_none()
                && url.path() != "/"
                && routes
                    .iter()
                    .any(|(_, path)| route_matches(url.path(), path, &BTreeSet::new()))
            {
                servers.insert(url.origin().ascii_serialization());
            }
        }
    }
    servers
}

fn safe_server(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.username().is_empty()
        && url.password().is_none()
}

/// OpenAPI can be published as JSON or YAML. Never interpret an ordinary page as a schema.
pub fn api_schema(text: &str) -> Option<serde_json::Value> {
    let document = serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .or_else(|| {
            if regex::Regex::new(r"(?m)^(?:openapi|swagger)\s*:")
                .unwrap()
                .is_match(text)
            {
                serde_yaml::from_str::<serde_json::Value>(text).ok()
            } else {
                None
            }
        })?;
    if document.get("openapi").is_some() || document.get("swagger").is_some() {
        Some(document)
    } else {
        None
    }
}

pub fn route_matches(path: &str, documented: &str, prefixes: &BTreeSet<String>) -> bool {
    fn shape(path: &str) -> String {
        regex::Regex::new(r"\{[^}]+\}")
            .unwrap()
            .replace_all(path, "{}")
            .into_owned()
    }
    fn matches(path: &str, template: &str) -> bool {
        if shape(path) == shape(template) {
            return true;
        }
        let variables = regex::Regex::new(r"\{[^}/]+\}").unwrap();
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn yaml_servers_resolve_defaults_variables_and_operation_overrides() {
        let source = Url::parse("https://docs.example.test:8443/reference/openapi.yaml").unwrap();
        let text = "openapi: 3.0.4\nservers:\n  - url: https://{tenant}.example.test/{version}\n    variables:\n      tenant: {default: api}\n      version: {default: v2}\npaths:\n  /items:\n    get:\n      servers:\n        - url: ../special\n";
        assert!(api_schema(text).is_some());
        let bases = api_bases(&source, text);
        assert!(bases.contains("https://api.example.test/v2"));
        assert!(bases.contains("https://docs.example.test:8443/special"));
        assert_eq!(
            api_bases(&source, "openapi: 3.0.4\npaths: {}"),
            BTreeSet::from(["https://docs.example.test:8443".into()])
        );
        assert_eq!(
            api_bases(&source, "swagger: '2.0'\nbasePath: /v1\npaths: {}"),
            BTreeSet::from(["https://docs.example.test:8443/v1".into()])
        );
        assert!(
            api_bases(
                &source,
                r#"{"openapi":"3.0.4","servers":[{"url":"https://user:secret@api.example.test"}]}"#
            )
            .is_empty()
        );
        assert!(api_schema("<html>openapi: is not a schema</html>").is_none());
    }
    #[test]
    fn api_servers_are_published_evidence_with_independent_origins() {
        let source = Url::parse("https://docs.example.net/openapi.json").unwrap();
        let bases = api_bases(
            &source,
            r#"{"openapi":"3.1.0","servers":[{"url":"https://api.example.com/v2"},{"url":"/local-api"}]}"#,
        );
        assert!(bases.contains("https://api.example.com/v2"));
        assert!(bases.contains("https://docs.example.net/local-api"));
        assert!(
            api_bases(&source, "<p>Base URL https://api.example.com/v3</p>")
                .contains("https://api.example.com/v3")
        );
        assert!(api_bases(&source, "<a href='https://unrelated.example.com'>Link</a>").is_empty());
        let examples = "GET /search/widgets\ncurl -H 'Accept: application/json' https://api.example.com/search/widgets?q=sample\nSee https://unrelated.example.net/elsewhere";
        let bases = api_bases(&source, examples);
        assert_eq!(bases, BTreeSet::from(["https://api.example.com".into()]));
        assert!(route_matches(
            "/teams/demo/items/123",
            "/teams/{team}/items/{item}",
            &BTreeSet::new()
        ));
        assert!(!route_matches(
            "/teams/demo/items/123/delete",
            "/teams/{team}/items/{item}",
            &BTreeSet::new()
        ));
    }
    #[test]
    fn follows_published_docs_across_hosts_and_markdown_indices() {
        let home = Url::parse("https://example.com/").unwrap();
        let links = published_links(
            &home,
            r#"<a href="https://docs.example.net/reference#start">Docs</a><a href="/logout">exit</a><script src="/assets/app.js"></script>"#,
        );
        assert!(links.contains("https://docs.example.net/reference"));
        assert!(!links.iter().any(|url| url.contains("logout")));
        let invalid = published_links(
            &home,
            r#"<a href='/resources/:type(.:format)'>Template</a><a href='/teams/{team}'>Template</a>"#,
        );
        assert!(invalid.is_empty());
        assert!(
            priority("https://docs.example.net/")
                > priority("https://example.com/resources/articles")
        );
        let docs = Url::parse("https://docs.example.net/llms.txt").unwrap();
        let index = published_links(
            &docs,
            "[Users](https://docs.example.net/resources/users.md)\n[Auth](/reference/authentication.md)",
        );
        assert_eq!(index.len(), 2);
        assert_eq!(
            ranked(links.iter())[0],
            "https://docs.example.net/reference"
        );
    }
    #[test]
    fn reference_routes_keep_methods_and_parameter_evidence() {
        let routes = documented_routes(
            "## Current user % GET /users/@me\n## Messages % GET /channels/{channel.id}/messages\n## Send % POST /channels/{channel.id}/messages",
        );
        assert!(routes.contains(&("GET".into(), "/users/@me".into())));
        assert!(routes.contains(&("POST".into(), "/channels/{channel.id}/messages".into())));
        assert!(!routes.contains(&("DELETE".into(), "/users/@me".into())));
        let base = Url::parse("https://example.com").unwrap();
        let prefixes = api_prefixes(
            &base,
            "Base https://example.com/api Version URL https://example.com/api/v{version_number} Version Status Default 10 Available 9 Available 8 Deprecated",
        );
        assert!(route_matches("/api/v10/users/@me", "/users/@me", &prefixes));
        assert!(!prefixes.contains("/api/v8"));
    }
    #[test]
    fn documentation_components_preserve_verbs_and_linked_parameters() {
        let text = crate::runtime::visible_text(
            r##"<Endpoint method="GET">/current-context</Endpoint>
<ApiRoute className="route" method='PATCH'>/teams/[\{team.id\}](/reference/team)/items/[\{item.id\}](/reference/item)</ApiRoute>"##,
        );
        let routes = documented_routes(&text);
        assert!(routes.contains(&("GET".into(), "/current-context".into())));
        assert!(routes.contains(&("PATCH".into(), "/teams/{team.id}/items/{item.id}".into())));
        assert_eq!(routes.len(), 2);
        let wrapped = crate::runtime::visible_text(
            r#"<span>get</span><span><wbr/>/teams<wbr/>/{team}<wbr/>/items</span><span>post</span><span><wbr>/teams<wbr>/{team}<wbr>/items</span>"#,
        );
        let routes = documented_routes(&wrapped);
        assert_eq!(
            routes,
            BTreeSet::from([
                ("GET".into(), "/teams/{team}/items".into()),
                ("POST".into(), "/teams/{team}/items".into())
            ])
        );
    }
}

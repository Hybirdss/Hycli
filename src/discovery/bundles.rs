use std::collections::BTreeSet;
use url::Url;

use super::schema::safe_server;

fn normalize_bundle_path(raw: &str) -> Option<String> {
    let mut cleaned = raw.trim().replace("\\/", "/");
    if let Some((before_q, _)) = cleaned.split_once('?') {
        cleaned = before_q.to_owned();
    }
    if let Some((before_semi, _)) = cleaned.split_once(';') {
        cleaned = before_semi.to_owned();
    }
    if let Some((before_hash, _)) = cleaned.split_once('#') {
        cleaned = before_hash.to_owned();
    }
    // Convert JS template expressions `${encodeURIComponent(slug)}` or `${item.id}` -> `{slug}` / `{id}`
    let template_re = regex::Regex::new(r"\$\{([^}]+)\}").expect("constant");
    cleaned = template_re
        .replace_all(&cleaned, |caps: &regex::Captures<'_>| {
            let expr = &caps[1];
            let ident = regex::Regex::new(r"[A-Za-z_][A-Za-z0-9_]*")
                .expect("constant")
                .find_iter(expr)
                .map(|m| m.as_str())
                .filter(|w| !matches!(*w, "encodeURIComponent" | "String" | "Number" | "this"))
                .last()
                .unwrap_or("id");
            format!("{{{ident}}}")
        })
        .into_owned();
    let colon_re = regex::Regex::new(r"/:([A-Za-z_][A-Za-z0-9_]*)").expect("constant");
    cleaned = colon_re.replace_all(&cleaned, "/{$1}").into_owned();
    let brace_re = regex::Regex::new(r"\{([^}]+)\}").expect("constant");
    cleaned = brace_re
        .replace_all(&cleaned, |caps: &regex::Captures<'_>| {
            let raw_id = caps[1].trim();
            let sanitized = if matches!(
                raw_id,
                "String" | "Number" | "encodeURIComponent" | "this" | "undefined" | "null"
            ) {
                "id".to_string()
            } else {
                raw_id
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() {
                            c.to_ascii_lowercase()
                        } else {
                            '_'
                        }
                    })
                    .collect::<String>()
                    .trim_matches('_')
                    .to_string()
            };
            let final_id = if crate::util::valid_id(&sanitized) {
                sanitized
            } else {
                "id".to_string()
            };
            format!("{{{final_id}}}")
        })
        .into_owned();
    let trimmed = cleaned.trim_end_matches('/').to_owned();
    let path = if trimmed.is_empty() {
        "/".to_owned()
    } else {
        trimmed
    };
    if !path.starts_with('/')
        || path.starts_with("//")
        || path == "/"
        || path.len() > 300
        || path.contains([' ', '\t', '\r', '\n', '<', '>', '"', '\'', '`', '\\', '*'])
    {
        return None;
    }
    let lower = path.to_ascii_lowercase();
    if [
        ".js", ".css", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".woff", ".woff2", ".ttf", ".ico",
        ".map", ".webp", ".mp4", ".mp3",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext))
    {
        return None;
    }
    Some(path)
}

/// Extract HTTP routes from inline `<script>` blocks and JS bundles (`fetch`, `axios`, `/api/...`, GraphQL).
pub fn bundle_routes(text: &str) -> BTreeSet<(String, String)> {
    let mut routes = BTreeSet::new();
    // Normalize JS string concatenation inside fetch(...) e.g.
    //    fetch('/api/entries/' + encodeURIComponent(id) + '/comments', { method: 'POST' })
    let concat_re = regex::Regex::new(
        r#"["'`]\s*\+\s*(?:(?:encodeURIComponent|String|Number)\s*\(\s*)?([A-Za-z_][A-Za-z0-9_.]*)\s*\)?\s*(?:\+\s*["'`])?"#,
    )
    .expect("constant");
    let normalized = concat_re
        .replace_all(text, |caps: &regex::Captures<'_>| {
            let raw_ident = caps[1].rsplit('.').next().unwrap_or("id");
            let ident = if matches!(
                raw_ident,
                "String" | "Number" | "encodeURIComponent" | "this"
            ) {
                "id".to_string()
            } else {
                raw_ident.to_ascii_lowercase()
            };
            let has_trailing_quote =
                caps[0].ends_with('"') || caps[0].ends_with('\'') || caps[0].ends_with('`');
            if has_trailing_quote {
                format!("{{{ident}}}")
            } else {
                format!("{{{ident}}}\"")
            }
        })
        .into_owned();

    let fetch_re = regex::Regex::new(
        r#"fetch\s*\(\s*["'`]((?:https?://[^/"'`\s]+)?/[^"'`\s]{1,260})["'`](?:\s*,\s*\{([^}]{0,320})\})?"#,
    )
    .expect("constant");
    let method_re =
        regex::Regex::new(r#"(?i)\bmethod\s*:\s*["'`](GET|HEAD|POST|PUT|PATCH|DELETE)["'`]"#)
            .expect("constant");
    for cap in fetch_re.captures_iter(&normalized).take(1000) {
        let raw_target = &cap[1];
        let raw_path = if raw_target.starts_with("http://") || raw_target.starts_with("https://") {
            let Ok(u) = Url::parse(&raw_target.replace('{', "_").replace('}', "_")) else {
                continue;
            };
            let Some(idx) = raw_target
                .find("://")
                .and_then(|i| raw_target[i + 3..].find('/').map(|j| i + 3 + j))
            else {
                continue;
            };
            let _ = u;
            &raw_target[idx..]
        } else {
            raw_target
        };
        let Some(path) = normalize_bundle_path(raw_path) else {
            continue;
        };
        let method = cap
            .get(2)
            .and_then(|opts| method_re.captures(opts.as_str()))
            .map(|m| m[1].to_ascii_uppercase())
            .unwrap_or_else(|| "GET".into());
        routes.insert((method, path));
    }

    let axios_re = regex::Regex::new(
        r#"(?i)\b(?:axios|client|http|api)\s*\.\s*(get|post|put|patch|delete|head)\s*\(\s*["'`]((?:https?://[^/"'`\s]+)?/[^"'`\s]{1,260})["'`]"#,
    )
    .expect("constant");
    for cap in axios_re.captures_iter(&normalized).take(1000) {
        let method = cap[1].to_ascii_uppercase();
        let raw_target = &cap[2];
        let raw_path = if let Some(idx) = raw_target
            .find("://")
            .and_then(|i| raw_target[i + 3..].find('/').map(|j| i + 3 + j))
        {
            &raw_target[idx..]
        } else {
            raw_target
        };
        if let Some(path) = normalize_bundle_path(raw_path) {
            routes.insert((method, path));
        }
    }

    let api_lit_re = regex::Regex::new(
        r#"["'`](/(?:_?api(?:/v[0-9]+)?|v[1-9][0-9]*|graphql|gql|rest|_next/data)(?:/[A-Za-z0-9_@.{}:$;=-]+)*)["'`]"#,
    )
    .expect("constant");
    for m in api_lit_re.captures_iter(&normalized).take(1500) {
        let Some(path) = normalize_bundle_path(&m[1]) else {
            continue;
        };
        let start = m.get(0).map(|x| x.start()).unwrap_or(0);
        let end = m.get(0).map(|x| x.end()).unwrap_or(0);
        let window_start = normalized.floor_char_boundary(start.saturating_sub(120));
        let window_end = normalized.ceil_char_boundary((end + 180).min(normalized.len()));
        let nearby = &normalized[window_start..window_end];
        if let Some(mc) = method_re.captures(nearby) {
            routes.insert((mc[1].to_ascii_uppercase(), path.clone()));
        } else if path.contains("graphql") || path.ends_with("/gql") {
            routes.insert(("POST".into(), path));
        } else if crate::policy::dangerous_intent(&path) {
            routes.insert(("POST".into(), path));
        } else {
            routes.insert(("GET".into(), path));
        }
    }

    // Match `new URL("/path", ...)` and Schema.org `urlTemplate` routes
    let new_url_re = regex::Regex::new(
        r#"(?:new\s+URL\s*\(\s*|["']urlTemplate["']\s*:\s*)["'`]((?:https?://[^/"'`\s]+)?/[^"'`\s]{1,260})["'`]"#,
    )
    .expect("constant");
    for cap in new_url_re.captures_iter(&normalized).take(500) {
        let raw_target = &cap[1];
        let raw_path = if let Some(idx) = raw_target
            .find("://")
            .and_then(|i| raw_target[i + 3..].find('/').map(|j| i + 3 + j))
        {
            &raw_target[idx..]
        } else {
            raw_target
        };
        if let Some(path) = normalize_bundle_path(raw_path) {
            routes.insert(("GET".into(), path));
        }
    }
    let search_lit_re = regex::Regex::new(
        r#"["'`](/(?:[a-z]{2}/)?search(?:/[A-Za-z0-9_.-]+)?)(?:\?[A-Za-z0-9_=&.-]*)?["'`]"#,
    )
    .expect("constant");
    for cap in search_lit_re.captures_iter(&normalized).take(200) {
        if let Some(path) = normalize_bundle_path(&cap[1]) {
            routes.insert(("GET".into(), path));
        }
    }

    let form_re = regex::Regex::new(r"(?is)<form\b([^>]*)>").expect("constant");
    let action_re = regex::Regex::new(
        r#"(?i)\baction\s*=\s*["']((?:https?://[^/"'\s]+)?/[^"'\s<>]{1,260})["']"#,
    )
    .expect("constant");
    let form_method_re =
        regex::Regex::new(r#"(?i)\bmethod\s*=\s*["'](GET|POST|PUT|PATCH|DELETE)["']"#)
            .expect("constant");
    for cap in form_re.captures_iter(text).take(200) {
        let attrs = &cap[1];
        let Some(act) = action_re.captures(attrs) else {
            continue;
        };
        let raw_target = &act[1];
        let raw_path = if let Some(idx) = raw_target
            .find("://")
            .and_then(|i| raw_target[i + 3..].find('/').map(|j| i + 3 + j))
        {
            &raw_target[idx..]
        } else {
            raw_target
        };
        if let Some(path) = normalize_bundle_path(raw_path) {
            let method = form_method_re
                .captures(attrs)
                .map(|m| m[1].to_ascii_uppercase())
                .unwrap_or_else(|| "GET".into());
            routes.insert((method, path));
        }
    }
    routes
}

fn bundle_priority(url: &str) -> usize {
    let lower = url.to_ascii_lowercase();
    if [
        "polyfill",
        "vendor",
        "webpack-",
        "framework-",
        "runtime-",
        "main-app-",
        "commons-",
        "fd9d1056-",
    ]
    .iter()
    .any(|low| lower.contains(low))
    {
        return 3;
    }
    if [
        "/app/",
        "/pages/",
        "/routes/",
        "/_remix/",
        "/page-",
        "/_index-",
        "/index-",
        "entry.client",
        "search",
        "query",
        "mutation",
        "graphql",
        "api",
        "manifest",
    ]
    .iter()
    .any(|hi| lower.contains(hi))
    {
        return 0;
    }
    2
}

/// Extract application JS bundle URLs from `<script src>`, `<link rel="modulepreload">`, and inline ES module imports.
pub fn bundle_urls(base: &Url, html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let script_re = regex::Regex::new(r#"(?is)<script\b[^>]*\bsrc\s*=\s*["']([^"'<>\s]+)["']"#)
        .expect("constant");
    let link_re = regex::Regex::new(
        r#"(?is)<link\b[^>]*\b(?:rel\s*=\s*["']modulepreload["']|as\s*=\s*["']script["'])[^>]*\bhref\s*=\s*["']([^"'<>\s]+)["']|<link\b[^>]*\bhref\s*=\s*["']([^"'<>\s]+)["'][^>]*\b(?:rel\s*=\s*["']modulepreload["']|as\s*=\s*["']script["'])"#,
    )
    .expect("constant");
    let import_re = regex::Regex::new(
        r#"\bimport\s+(?:[^"'`;\n]{1,120}\s+from\s+)?["']((?:https?://[^/"'\s]+)?/[^"'\s]+\.m?js(?:\?[^"'\s]*)?)["']"#,
    )
    .expect("constant");

    let raw_candidates = script_re
        .captures_iter(html)
        .take(120)
        .filter_map(|c| c.get(1).map(|m| m.as_str()))
        .chain(
            link_re
                .captures_iter(html)
                .take(120)
                .filter_map(|c| c.get(1).or_else(|| c.get(2)).map(|m| m.as_str())),
        )
        .chain(
            import_re
                .captures_iter(html)
                .take(120)
                .filter_map(|c| c.get(1).map(|m| m.as_str())),
        );

    for raw_match in raw_candidates {
        let raw = raw_match.replace("&amp;", "&");
        let Ok(mut url) = base.join(&raw) else {
            continue;
        };
        if !matches!(url.scheme(), "http" | "https") {
            continue;
        };
        url.set_fragment(None);
        let lower = url.as_str().to_ascii_lowercase();
        if [
            "googletagmanager",
            "google-analytics",
            "doubleclick",
            "clarity.ms",
            "hotjar",
            "sentry",
            "facebook.net",
            "connect.facebook",
            "amplitude.com",
            "datadoghq",
            "braze",
            "intercom",
            "channel.io",
            "wcs.naver.net",
            "kakao.com/sdk",
        ]
        .iter()
        .any(|skip| lower.contains(skip))
        {
            continue;
        }
        let path_lower = url.path().to_ascii_lowercase();
        if crate::net::same_site_origin(&url, base)
            || path_lower.ends_with(".js")
            || path_lower.ends_with(".mjs")
            || lower.contains("/_next/static/")
            || lower.contains("/_remix/")
            || lower.contains("/assets/")
        {
            let s = url.to_string();
            if seen.insert(s.clone()) {
                out.push(s);
            }
        }
        if out.len() >= 80 {
            break;
        }
    }
    let base_pri: Vec<usize> = out.iter().map(|u| bundle_priority(u)).collect();
    let mut indexed: Vec<(usize, String)> = out.into_iter().enumerate().collect();
    indexed.sort_by_key(|(i, _)| {
        let p = base_pri[*i];
        if p == 2 && (base_pri.get(i + 1) == Some(&0) || base_pri.get(i + 2) == Some(&0)) {
            1
        } else {
            p
        }
    });
    let mut out: Vec<String> = indexed.into_iter().map(|(_, u)| u).collect();
    out.truncate(24);
    out
}

/// Extract same-site API server origins referenced inside HTML or JS bundles.
pub fn bundle_api_bases(base: &Url, text: &str) -> BTreeSet<String> {
    let mut bases = BTreeSet::new();
    let url_re = regex::Regex::new(
        r#"https?://[A-Za-z0-9.-]+(?::[0-9]+)?(?:/api(?:/v[0-9]+)?|/v[1-9][0-9]*|/graphql|/gql|/rest)?"#,
    )
    .expect("constant");
    for m in url_re.find_iter(text).take(1000) {
        if let Ok(url) = Url::parse(m.as_str()) {
            if !safe_server(&url) || !crate::net::same_site_origin(&url, base) {
                continue;
            }
            let host = url.host_str().unwrap_or("").to_ascii_lowercase();
            if [
                "images.", "img.", "cdn.", "assets.", "static.", "media.", "cache.", "fonts.",
            ]
            .iter()
            .any(|p| host.starts_with(p))
            {
                continue;
            }
            let has_api_path = url.path() != "/" && !url.path().is_empty();
            let has_api_host = [
                "api.",
                "api-",
                "v1.",
                "v2.",
                "v3.",
                "gql.",
                "graphql.",
                "gateway.",
                "backend.",
                "studio-api",
            ]
            .iter()
            .any(|p| host.starts_with(p));
            if has_api_path || has_api_host || url.origin() == base.origin() {
                bases.insert(url.origin().ascii_serialization());
            }
        }
    }
    bases
}

use super::dom::stable_name;
use scraper::{Html, Selector};
use serde_json::{Value, json};

/// Extract same-origin category/feed navigation paths from `<nav>` for server-rendered list pages.
pub fn nav_list_routes(raw: &str, base: &str) -> Vec<String> {
    let document = Html::parse_document(raw);
    let Ok(base_url) = url::Url::parse(base) else {
        return Vec::new();
    };
    let Ok(sel) = Selector::parse("header nav a[href], nav a[href]") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for el in document.select(&sel).take(40) {
        let Some(href) = el.value().attr("href") else {
            continue;
        };
        let Ok(u) = base_url.join(href) else {
            continue;
        };
        if u.origin() != base_url.origin() || u.query().is_some() {
            continue;
        }
        let p = u.path();
        if p == "/"
            || p.len() > 36
            || !p
                .trim_matches('/')
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            continue;
        }
        let lower = p.to_ascii_lowercase();
        if [
            "/login",
            "/signin",
            "/signup",
            "/register",
            "/logout",
            "/write",
            "/submit",
            "/search",
            "/settings",
            "/about",
            "/faq",
            "/terms",
            "/privacy",
            "/support",
            "/sponsor",
            "/guidelines",
            "/start",
            "/bookmarklet",
            "/geekbots",
            "/geekbadge",
            "/lists",
            "/threads",
            "/comments",
            "/past",
            "/weekly",
            "/articles",
        ]
        .contains(&lower.as_str())
        {
            continue;
        }
        if !out.contains(&p.to_string()) {
            out.push(p.to_string());
            if out.len() >= 6 {
                break;
            }
        }
    }
    out
}

/// Structural form descriptors (action, method, and named input/select/textarea controls) without field values.
pub fn forms(html: &str) -> Value {
    let document = Html::parse_document(html);
    let mut out = Vec::new();
    let Ok(form_sel) = Selector::parse("form") else {
        return json!(out);
    };
    let control_sel = Selector::parse("input[name], select[name], textarea[name]").unwrap();
    for form in document.select(&form_sel).take(40) {
        let action = form.value().attr("action").unwrap_or("").trim();
        let method = form
            .value()
            .attr("method")
            .unwrap_or("GET")
            .trim()
            .to_ascii_uppercase();
        let mut inputs = Vec::new();
        for ctrl in form.select(&control_sel).take(60) {
            let Some(name) = ctrl.value().attr("name").filter(|n| stable_name(n)) else {
                continue;
            };
            let tag = ctrl.value().name();
            let kind =
                ctrl.value()
                    .attr("type")
                    .unwrap_or(if tag == "input" { "text" } else { tag });
            let required = ctrl.value().attr("required").is_some();
            inputs.push(json!({
                "name": name,
                "type": kind,
                "required": required,
            }));
        }
        out.push(json!({
            "action": action,
            "method": method,
            "inputs": inputs,
        }));
    }
    json!(out)
}

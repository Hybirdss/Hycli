use super::extract::Field;
use scraper::{Html, Selector};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Bounded DOM skeleton for choosing selectors. Values from hidden inputs, inline scripts,
/// storage and data attributes are intentionally omitted; visible text is supplied separately.
pub fn structure(html: &str) -> Value {
    let document = Html::parse_document(html);
    let mut nodes = Vec::new();
    for element in document.select(&Selector::parse("*").unwrap()).take(600) {
        let tag = element.value().name();
        if matches!(tag, "script" | "style" | "meta" | "input" | "link") {
            continue;
        }
        let classes = element
            .value()
            .classes()
            .filter(|value| stable_name(value))
            .take(6)
            .collect::<Vec<_>>();
        let id = element.value().id().filter(|value| stable_name(value));
        let mut selector = tag.to_string();
        if let Some(id) = id {
            append_selector_value(&mut selector, "id", id);
        }
        for class in classes {
            append_selector_value(&mut selector, "class", class);
        }
        let count = Selector::parse(&selector)
            .ok()
            .map(|selector| document.select(&selector).count())
            .unwrap_or(0);
        let attributes = ["href", "src", "title", "alt", "datetime", "content"]
            .into_iter()
            .filter(|attribute| element.value().attr(attribute).is_some())
            .collect::<Vec<_>>();
        let parent = element
            .parent()
            .and_then(scraper::ElementRef::wrap)
            .map(|parent| {
                let mut name = parent.value().name().to_string();
                for class in parent
                    .value()
                    .classes()
                    .filter(|value| stable_name(value))
                    .take(3)
                {
                    append_selector_value(&mut name, "class", class);
                }
                name
            });
        let node =
            json!({"selector":selector,"matches":count,"attributes":attributes,"parent":parent});
        if !nodes.contains(&node) {
            nodes.push(node);
        }
        if nodes.len() >= 100 {
            break;
        }
    }
    json!(nodes)
}

pub(super) fn stable_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_:.".contains(&b))
}

fn append_selector_value(selector: &mut String, attribute: &str, value: &str) {
    // Punctuation and a leading digit are valid HTML names but need CSS escaping.
    // stable_name excludes quotes, so an attribute selector safely preserves them.
    if value
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    {
        selector.push(if attribute == "id" { '#' } else { '.' });
        selector.push_str(value);
    } else {
        let operator = if attribute == "class" { "~=" } else { "=" };
        selector.push_str(&format!("[{attribute}{operator}\"{value}\"]"));
    }
}

pub(super) fn element_text(element: scraper::ElementRef<'_>) -> String {
    element
        .descendants()
        .filter_map(|node| {
            let text = node.value().as_text()?;
            let hidden = node
                .ancestors()
                .filter_map(scraper::ElementRef::wrap)
                .any(|parent| {
                    matches!(
                        parent.value().name(),
                        "script" | "style" | "noscript" | "template"
                    ) || parent.value().attr("hidden").is_some()
                        || parent.value().attr("aria-hidden") == Some("true")
                });
            (!hidden).then_some(text.as_ref())
        })
        .collect::<Vec<&str>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Describe selectable fields without exposing their values, including CSRF/session values.
/// The agent chooses the identity mapping; no website name or identity selector is built in.
pub fn fields(html: &str) -> Value {
    let document = Html::parse_document(html);
    let mut fields = Vec::new();
    let mut seen = BTreeSet::new();
    for element in document.select(&Selector::parse("*").unwrap()) {
        let tag = element.value().name();
        let mut candidates = Vec::new();
        if let Some(name) = element
            .value()
            .attr("name")
            .filter(|name| matches!(tag, "meta" | "input") && stable_name(name))
        {
            let lower = name.to_ascii_lowercase();
            if ![
                "token",
                "secret",
                "password",
                "csrf",
                "authenticity",
                "nonce",
            ]
            .iter()
            .any(|part| lower.contains(part))
                && element.value().attr("type") != Some("password")
            {
                candidates.push(Field {
                    selector: format!("{tag}[name=\"{name}\"]"),
                    attribute: if tag == "meta" { "content" } else { "value" }.into(),
                });
            }
        }
        for (name, _) in element.value().attrs().filter(|(name, _)| {
            name.starts_with("data-") && (name.contains("user") || name.contains("account"))
        }) {
            if stable_name(name) {
                candidates.push(Field {
                    selector: format!("{tag}[{name}]"),
                    attribute: name.into(),
                });
            }
        }
        for field in candidates {
            if seen.insert((field.selector.clone(), field.attribute.clone())) {
                fields.push(json!({"selector":field.selector,"attribute":field.attribute,"unique_nonempty":field.extract(&document).is_some()}));
            }
        }
        if fields.len() >= 100 {
            break;
        }
    }
    json!(fields)
}

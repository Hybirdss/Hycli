//! Bounded structural HTML evidence and declarative extraction, without script execution.
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Field {
    pub selector: String,
    /// Empty selects text; otherwise selects one named attribute.
    pub attribute: String,
}

impl Field {
    pub fn extract(&self, document: &Html) -> Option<String> {
        self.extract_bounded(document, 320)
    }
    /// Response guards check structure, not the size limit of an account identity.
    pub fn is_present(&self, document: &Html) -> bool {
        self.extract_bounded(document, usize::MAX).is_some()
    }
    fn extract_bounded(&self, document: &Html, max_length: usize) -> Option<String> {
        if self.selector.is_empty() || self.selector.len() > 500 || self.attribute.len() > 80 {
            return None;
        }
        let selector = Selector::parse(&self.selector).ok()?;
        let mut matches = document.select(&selector);
        let element = matches.next()?;
        // Never silently choose a person from a list of profiles or an account switcher.
        if matches.next().is_some() {
            return None;
        }
        let value = if self.attribute.is_empty() {
            element_text(element)
        } else {
            element.value().attr(&self.attribute)?.to_string()
        };
        let value = value.trim();
        if value.is_empty() || value.len() > max_length || value.contains(['\r', '\n']) {
            None
        } else {
            Some(value.into())
        }
    }
}

/// Repeated page records and optional page-level fields, declared from observed DOM evidence.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Extraction {
    pub items: String,
    pub fields: std::collections::BTreeMap<String, OutputField>,
    pub page: std::collections::BTreeMap<String, OutputField>,
    /// A hard output bound, never an instruction to fetch more pages.
    pub limit: usize,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OutputField {
    /// Empty selects the record itself; nonempty selectors are relative to the record.
    pub selector: String,
    pub attribute: String,
    pub required: bool,
    pub absolute_url: bool,
}
impl Extraction {
    pub fn valid(&self) -> bool {
        (!self.fields.is_empty() || !self.page.is_empty())
            && self.fields.len() + self.page.len() <= 40
            && self.limit <= 1000
            && self.items.len() <= 500
            && (self.items.is_empty() || Selector::parse(&self.items).is_ok())
            && self
                .fields
                .iter()
                .chain(self.page.iter())
                .all(|(name, field)| {
                    stable_name(name)
                        && !crate::util::secret_field(name)
                        && field.selector.len() <= 500
                        && field.attribute.len() <= 80
                        && (field.selector.is_empty() || Selector::parse(&field.selector).is_ok())
                        && !crate::util::secret_field(&field.attribute)
                })
    }
    pub fn extract(&self, html: &str, base: &str) -> Option<Value> {
        if !self.valid() {
            return None;
        }
        let document = Html::parse_document(html);
        let base = url::Url::parse(base).ok()?;
        let mut output = extract_fields(document.root_element(), &self.page, &base)?;
        if !self.fields.is_empty() {
            let records = if self.items.is_empty() {
                vec![document.root_element()]
            } else {
                document
                    .select(&Selector::parse(&self.items).ok()?)
                    .take(if self.limit == 0 { 100 } else { self.limit })
                    .collect()
            };
            let items: Option<Vec<_>> = records
                .into_iter()
                .map(|record| extract_fields(record, &self.fields, &base).map(Value::Object))
                .collect();
            output.insert("items".into(), Value::Array(items?));
        }
        Some(Value::Object(output))
    }
}
fn extract_fields(
    root: scraper::ElementRef<'_>,
    fields: &std::collections::BTreeMap<String, OutputField>,
    base: &url::Url,
) -> Option<serde_json::Map<String, Value>> {
    let mut output = serde_json::Map::new();
    for (name, field) in fields {
        let element = if field.selector.is_empty() {
            Some(root)
        } else {
            let selector = Selector::parse(&field.selector).ok()?;
            let mut matches = root.select(&selector);
            let first = matches.next();
            if matches.next().is_some() {
                None
            } else {
                first
            }
        };
        let value = element
            .and_then(|element| {
                if field.attribute.is_empty() {
                    Some(element_text(element))
                } else {
                    element.value().attr(&field.attribute).map(str::to_owned)
                }
            })
            .filter(|value| !value.is_empty() && value.len() <= 16_384);
        let value = if field.absolute_url {
            value
                .and_then(|value| base.join(&value).ok())
                .filter(|url| {
                    matches!(url.scheme(), "http" | "https")
                        && url.username().is_empty()
                        && url.password().is_none()
                })
                .map(|url| url.to_string())
        } else {
            value
        };
        if field.required && value.is_none() {
            return None;
        }
        output.insert(
            name.clone(),
            value.map(Value::String).unwrap_or(Value::Null),
        );
    }
    Some(output)
}

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

fn stable_name(value: &str) -> bool {
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

fn element_text(element: scraper::ElementRef<'_>) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extraction_refuses_missing_or_ambiguous_required_fields() {
        let extraction: Extraction = serde_json::from_value(json!({"items":"article","fields":{"title":{"selector":"h2","required":true},"url":{"selector":"a","attribute":"href","absolute_url":true}}})).unwrap();
        assert!(
            extraction
                .extract(
                    "<article><h2>One</h2><h2>Two</h2></article>",
                    "https://example.test/list"
                )
                .is_none()
        );
        let value = extraction
            .extract(
                "<article><h2>A\n title</h2><a href='javascript:alert(1)'>Link</a></article>",
                "https://example.test/list",
            )
            .unwrap();
        assert_eq!(value["items"][0]["title"], "A title");
        assert!(value["items"][0]["url"].is_null());
        let evidence = structure(
            "<article class='book'><a href='/item'>Read</a><input type='hidden' value='private-value'><script>secret_token='private-value'</script></article>",
        );
        assert!(evidence.to_string().contains("article.book"));
        assert!(!evidence.to_string().contains("private-value"));
        let evidence =
            structure("<article id='12:item' class='sm:wide book.item'><h2>Title</h2></article>");
        for node in evidence.as_array().unwrap() {
            assert!(
                node["matches"].as_u64().unwrap() > 0,
                "{}",
                node["selector"]
            );
        }
        let value = extraction.extract("<article><h2>A\n title<script>private-value</script><span hidden>hidden-value</span><style>private-style</style></h2></article>", "https://example.test/list").unwrap();
        assert_eq!(value["items"][0]["title"], "A title");
        let identity = Field {
            selector: "h2".into(),
            attribute: String::new(),
        };
        assert_eq!(
            identity.extract(&Html::parse_document("<h2>Mina\n Park</h2>")),
            Some("Mina Park".into())
        );
        let long_page = Html::parse_document(&format!("<h2>{}</h2>", "Book title ".repeat(100)));
        assert!(identity.extract(&long_page).is_none());
        assert!(identity.is_present(&long_page));
        assert!(!identity.is_present(&Html::parse_document("<h2>One</h2><h2>Two</h2>")));
    }
    #[test]
    fn fields_keep_values_private_and_refuse_ambiguous_identity() {
        let html = r#"<meta name="current-person" content="person-private"><meta name="csrf-token" content="secret-private"><input name="email" value="private@example.test"><span data-account-id="17"></span>"#;
        let evidence = fields(html).to_string();
        assert!(evidence.contains("current-person"));
        assert!(evidence.contains("data-account-id"));
        for value in [
            "person-private",
            "private@example.test",
            "secret-private",
            "csrf-token",
        ] {
            assert!(!evidence.contains(value));
        }
        let field = Field {
            selector: "meta[name=\"current-person\"]".into(),
            attribute: "content".into(),
        };
        assert_eq!(
            field.extract(&Html::parse_document(html)).as_deref(),
            Some("person-private")
        );
        assert!(
            field
                .extract(&Html::parse_document(&format!("{html}{html}")))
                .is_none()
        );
        assert!(
            field
                .extract(&Html::parse_document(
                    "<meta name='current-person' content=''>"
                ))
                .is_none()
        );
    }
}

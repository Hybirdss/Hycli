use super::dom::{element_text, stable_name};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Extraction {
    pub items: String,
    pub fields: std::collections::BTreeMap<String, OutputField>,
    pub page: std::collections::BTreeMap<String, OutputField>,
    /// A hard output bound, never an instruction to fetch more pages.
    pub limit: usize,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
                let matched: Vec<_> = document
                    .select(&Selector::parse(&self.items).ok()?)
                    .take(if self.limit == 0 { 100 } else { self.limit })
                    .collect();
                if matched.is_empty() {
                    return None;
                }
                matched
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

/// Deterministically infer a repeating DOM record `Extraction` when an HTML page contains >= 3 structured item rows.
pub fn infer_html_extraction(raw: &str, base: &str) -> Option<Extraction> {
    let document = Html::parse_document(raw);
    let _base_url = url::Url::parse(base).ok()?;
    let mut candidates = vec![
        "div.topic_row".to_string(),
        "tr.athing".to_string(),
        "article".to_string(),
        "li.story".to_string(),
        "div.post-item".to_string(),
        "div.search-result".to_string(),
        "div.result".to_string(),
        "div.item".to_string(),
        "li.item".to_string(),
        "li.post".to_string(),
    ];
    if let Ok(scan_sel) = Selector::parse("main *[class], body *[class]") {
        for el in document.select(&scan_sel).take(600) {
            let tag = el.value().name();
            if !matches!(tag, "div" | "li" | "article" | "tr") {
                continue;
            }
            if let Some(cls) = el.value().classes().find(|c| {
                !c.is_empty()
                    && c.len() <= 40
                    && c.chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
                    && [
                        "row", "item", "topic", "post", "card", "story", "result", "entry",
                        "article", "thread",
                    ]
                    .iter()
                    .any(|kw| c.to_ascii_lowercase().contains(kw))
            }) {
                let cand = format!("{tag}.{cls}");
                if !candidates.contains(&cand) {
                    candidates.push(cand);
                }
            }
        }
    }

    let pick_field = |records: &[scraper::ElementRef<'_>],
                      opts: &[(&str, &str)],
                      abs_url: bool|
     -> Option<OutputField> {
        let min_hits = (records.len() * 4).div_ceil(5).max(2);
        for &(sel_str, attr_str) in opts {
            let parsed_sel = if sel_str.is_empty() {
                None
            } else {
                let Ok(s) = Selector::parse(sel_str) else {
                    continue;
                };
                Some(s)
            };
            let mut hits = 0usize;
            for rec in records {
                let target = if let Some(ref s) = parsed_sel {
                    let mut m = rec.select(s);
                    let first = m.next();
                    if m.next().is_some() { None } else { first }
                } else {
                    Some(*rec)
                };
                let val = target
                    .and_then(|el| {
                        if attr_str.is_empty() {
                            Some(element_text(el))
                        } else {
                            el.value().attr(attr_str).map(str::to_owned)
                        }
                    })
                    .filter(|v| !v.trim().is_empty() && v.len() <= 16_384);
                if val.is_some() {
                    hits += 1;
                }
            }
            if hits >= min_hits {
                return Some(OutputField {
                    selector: sel_str.to_string(),
                    attribute: attr_str.to_string(),
                    required: false,
                    absolute_url: abs_url,
                });
            }
        }
        None
    };

    for items_sel in candidates {
        let Ok(sel) = Selector::parse(&items_sel) else {
            continue;
        };
        let records: Vec<_> = document.select(&sel).take(40).collect();
        if records.len() < 3 {
            continue;
        }
        let mut fields = std::collections::BTreeMap::new();
        if let Some(f) = pick_field(
            &records,
            &[("", "data-topic-state-id"), ("", "data-id"), ("", "id")],
            false,
        ) {
            fields.insert("id".into(), f);
        }
        if let Some(f) = pick_field(
            &records,
            &[
                ("h1", ""),
                ("h2", ""),
                ("h3", ""),
                ("h4", ""),
                ("a.topic-title-link", ""),
                (".topictitle a", ""),
                (".titleline > a", ""),
                (".title a", ""),
                ("a[class*='title']", ""),
                ("a", ""),
            ],
            false,
        ) {
            fields.insert("title".into(), f);
        }
        if let Some(f) = pick_field(
            &records,
            &[
                ("a.topic-title-link", "href"),
                (".topictitle a", "href"),
                (".titleline > a", "href"),
                ("h1 a", "href"),
                ("h2 a", "href"),
                ("h3 a", "href"),
                (".title a", "href"),
                ("a[class*='title']", "href"),
                ("a[href]", "href"),
            ],
            true,
        ) {
            fields.insert("url".into(), f);
        }
        if !fields.contains_key("title") || !fields.contains_key("url") {
            continue;
        }
        if let Some(f) = pick_field(
            &records,
            &[
                (".topicdesc", ""),
                (".summary", ""),
                (".description", ""),
                (".excerpt", ""),
                (".snippet", ""),
                ("p", ""),
            ],
            false,
        ) {
            fields.insert("summary".into(), f);
        }
        if let Some(f) = pick_field(
            &records,
            &[
                (".topicinfo", ""),
                (".subtext", ""),
                (".meta", ""),
                (".byline", ""),
            ],
            false,
        ) {
            fields.insert("meta".into(), f);
        }
        if let Some(f) = pick_field(
            &records,
            &[("time[datetime]", "datetime"), ("time", "")],
            false,
        ) {
            fields.insert("published_at".into(), f);
        }
        let extraction = Extraction {
            items: items_sel,
            fields,
            page: std::collections::BTreeMap::new(),
            limit: 50,
        };
        if extraction.valid() && extraction.extract(raw, base).is_some() {
            return Some(extraction);
        }
    }
    None
}

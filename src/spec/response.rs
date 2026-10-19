use serde_json::Value;

use super::types::Operation;

impl Operation {
    pub fn graphql_query(&self, args: &std::collections::BTreeMap<String, String>) -> bool {
        let query = args.get("query").map(String::as_str).or_else(|| {
            self.query
                .get("query")
                .or_else(|| {
                    self.body
                        .as_ref()
                        .and_then(|body| body.json.get("query").or_else(|| body.form.get("query")))
                })?
                .default
                .as_ref()?
                .as_str()
        });
        query.is_some_and(|query| graphql_parser::parse_query::<&str>(query).is_ok())
    }
    pub fn parse_json_response(&self, response: &crate::net::Response) -> Option<Value> {
        let expected = self.response.as_ref();
        if let Some(exp) = expected {
            if !exp.ssr_json.is_empty() {
                let html = String::from_utf8_lossy(&response.bytes);
                return crate::html::extract_ssr_json(&html, &exp.ssr_json, &exp.extract_pointer);
            }
        }
        let mut parsed = response.json().or_else(|| {
            let text = std::str::from_utf8(&response.bytes).ok()?.trim_start();
            let stripped = if let Some(exp) = expected {
                if !exp.strip_prefix.is_empty() {
                    text.strip_prefix(exp.strip_prefix.as_str())
                        .map(str::trim_start)
                } else {
                    None
                }
            } else {
                None
            }
            .or_else(|| {
                for prefix in [")]}',", ")]}'", "while(1);", "for(;;);"] {
                    if let Some(rest) = text.strip_prefix(prefix) {
                        return Some(rest.trim_start());
                    }
                }
                None
            })?;
            serde_json::from_str(stripped).ok()
        })?;
        if let Some(exp) = expected {
            if !exp.extract_pointer.is_empty() {
                parsed = crate::html::resolve_pointer(&parsed, &exp.extract_pointer)?.clone();
            }
        }
        Some(parsed)
    }
    pub fn matches_response(&self, response: &crate::net::Response) -> bool {
        if !response.successful() {
            return false;
        }
        let Some(expected) = &self.response else {
            return true;
        };
        match expected.format.as_str() {
            "json" => self.parse_json_response(response).is_some_and(|body| {
                expected
                    .required_pointers
                    .iter()
                    .all(|p| crate::html::resolve_pointer(&body, p).is_some())
            }),
            "html" => {
                if !expected.ssr_json.is_empty() {
                    return response.header("content-type").contains("text/html")
                        && self.parse_json_response(response).is_some_and(|body| {
                            expected
                                .required_pointers
                                .iter()
                                .all(|p| crate::html::resolve_pointer(&body, p).is_some())
                        });
                }
                (!expected.required_html_fields.is_empty() || expected.html.is_some())
                    && response.header("content-type").contains("text/html")
                    && !response.bytes.is_empty()
                    && {
                        let document = scraper::Html::parse_document(&String::from_utf8_lossy(
                            &response.bytes,
                        ));
                        expected
                            .required_html_fields
                            .iter()
                            .all(|field| field.is_present(&document))
                            && expected.html.as_ref().is_none_or(|extraction| {
                                extraction
                                    .extract(
                                        &String::from_utf8_lossy(&response.bytes),
                                        &response.url,
                                    )
                                    .is_some()
                            })
                    }
            }
            "text" => {
                response.header("content-type").starts_with("text/") && !response.bytes.is_empty()
            }
            "xml" => response.header("content-type").contains("xml") && !response.bytes.is_empty(),
            "empty" => response.bytes.is_empty(),
            _ => false,
        }
    }
}

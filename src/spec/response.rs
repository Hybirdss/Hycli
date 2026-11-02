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
        query.is_some_and(|query| crate::policy::parse_graphql_bounded(query).is_some())
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
    pub(crate) fn matches_file_response(&self, received: &crate::net::FileResponse) -> bool {
        let Some(artifact) = received.artifact.as_ref() else {
            return false;
        };
        let response = &received.response;
        if !response.successful()
            || response.status == 206
            || !response.header("content-range").is_empty()
        {
            return false;
        }
        let Some(expected) = self
            .response
            .as_ref()
            .filter(|expected| expected.format == "file")
        else {
            return false;
        };
        let Some(file) = expected.file.as_ref() else {
            return false;
        };
        let media = response
            .header("content-type")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let prefix = String::from_utf8_lossy(artifact.prefix())
            .trim_start()
            .to_ascii_lowercase();
        let html = media == "text/html" || media == "application/xhtml+xml";
        let looks_html = ["<!doctype html", "<html", "<form"]
            .iter()
            .any(|start| prefix.starts_with(start));
        // Headers and a finite prefix detect obvious mismatches, not whole-file
        // safety or semantics. Every saved file remains untrusted and uninspected.
        let matches_type = if html {
            response
                .header("content-disposition")
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("attachment")
        } else {
            !looks_html
        };
        file.media_types.contains(&media)
            && (file.allow_empty || artifact.byte_count() != 0)
            && matches_type
    }

    pub fn matches_response(&self, response: &crate::net::Response) -> bool {
        if !response.successful() {
            return false;
        }
        let Some(expected) = &self.response else {
            return true;
        };
        match expected.format.as_str() {
            // File contracts require EOF-sealed reception, never a buffered JSON
            // shape or an inspection prefix pretending to be a complete file.
            "file" => false,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{net::Response, spec};

    fn response(status: u16, body: serde_json::Value) -> Response {
        Response {
            url: "https://example.test/api/report/revenue".into(),
            status,
            headers: vec![("content-type".into(), "application/json".into())],
            bytes: serde_json::to_vec(&body).unwrap(),
            duration_ms: 1,
            defense: "none".into(),
        }
    }

    fn daily_revenue_operation() -> Operation {
        spec::parse(
            serde_json::json!({
                "spec_version": 2,
                "site": {"name": "fixture", "base_url": "https://example.test"},
                "operations": [{
                    "name": "read-daily-revenue",
                    "method": "GET",
                    "path": "/api/report/revenue",
                    "effect": "read",
                    "evidence": "Synthetic daily revenue contract",
                    "response": {
                        "format": "json",
                        "required_pointers": ["/0/date", "/0/revenueAmount"]
                    }
                }]
            })
            .to_string()
            .as_bytes(),
        )
        .unwrap()
        .operations
        .remove(0)
    }

    #[test]
    fn daily_revenue_never_turns_auth_failure_or_missing_date_into_zero() {
        let operation = daily_revenue_operation();
        assert!(
            !operation.matches_response(&response(401, serde_json::json!({"revenueAmount": 0})))
        );
        assert!(!operation.matches_response(&response(200, serde_json::json!([]))));
        assert!(
            !operation.matches_response(&response(200, serde_json::json!([{"revenueAmount": 0}])))
        );
        assert!(operation.matches_response(&response(
            200,
            serde_json::json!([{"date": "20261017", "revenueAmount": 0}])
        )));
    }
}

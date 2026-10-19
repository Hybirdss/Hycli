//! Minimal HAR 1.2 parser for capture inputs.
use serde::Deserialize;

use crate::apperr::{self, AppResult};

#[derive(Debug, Deserialize)]
pub struct File {
    pub log: Log,
}

#[derive(Debug, Deserialize)]
pub struct Log {
    #[serde(default)]
    pub entries: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
pub struct Entry {
    pub request: Req,
    pub response: Resp,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct NameValue {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct PostData {
    #[serde(default, rename = "mimeType")]
    pub mime_type: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub params: Vec<NameValue>,
}

#[derive(Debug, Deserialize)]
pub struct Req {
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers: Vec<NameValue>,
    #[serde(default, rename = "queryString")]
    pub query_string: Vec<NameValue>,
    #[serde(default, rename = "postData")]
    pub post_data: Option<PostData>,
}

#[derive(Debug, Deserialize)]
pub struct Resp {
    #[serde(default)]
    pub status: u16,
    #[serde(default)]
    pub content: Content,
}

#[derive(Debug, Default, Deserialize)]
pub struct Content {
    #[serde(default, rename = "mimeType")]
    pub mime_type: String,
    #[serde(default)]
    pub text: String,
}

pub fn parse(data: &[u8]) -> AppResult<File> {
    serde_json::from_slice(data).map_err(|e| {
        apperr::spec(
            format!("Failed to parse HAR ({e})"),
            "Verify HAR 1.2 JSON format.",
        )
    })
}

//! HAR 최소 파서 — capture의 관찰 원료.
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

#[derive(Debug, Deserialize)]
pub struct Req {
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub url: String,
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
    serde_json::from_slice(data)
        .map_err(|e| apperr::spec(format!("HAR 파싱 실패 ({e})"), "HAR 1.2 형식 확인"))
}

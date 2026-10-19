use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SPEC_VERSION: i64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spec {
    #[serde(default)]
    pub spec_version: i64,
    pub site: SiteInfo,
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub auth: Vec<AuthStrategy>,
    #[serde(default)]
    pub operations: Vec<Operation>,
    #[serde(default)]
    pub models: std::collections::BTreeMap<String, Model>,
    #[serde(default)]
    pub health: Option<Health>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteInfo {
    pub name: String,
    #[serde(default)]
    pub title: String,
    pub base_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_url: String,
}
impl SiteInfo {
    pub fn source_url(&self) -> &str {
        if self.source_url.is_empty() {
            &self.base_url
        } else {
            &self.source_url
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Defaults {
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub timeout: String,
    #[serde(default)]
    pub tls_profile: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthStrategy {
    pub name: String,
    pub kind: String, // none | header | browser | derived
    #[serde(default)]
    pub header: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default, rename = "value_from")]
    pub value_from: String, // store:<site>/<key> | cookie:<name> | storage:<key>[#<pointer>]
    /// Exact credential destination; legacy definitions use the site's base origin.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origin: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreflightStep {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub op: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub method: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub base_url: String,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub args: std::collections::BTreeMap<String, String>,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub extract: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Operation {
    pub name: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub method: String,
    pub path: String,
    /// Optional evidenced server for sites whose web UI and API use different origins.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub transport: String,
    #[serde(default)]
    pub auth: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preflight: Vec<PreflightStep>,
    #[serde(default)]
    pub params: std::collections::BTreeMap<String, Param>,
    #[serde(default)]
    pub query: std::collections::BTreeMap<String, Param>,
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<Body>,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub effect: crate::policy::Effect,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub evidence: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<ResponseExpectation>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ResponseExpectation {
    pub format: String,
    pub required_pointers: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub required_html_fields: Vec<crate::html::Field>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub html: Option<crate::html::Extraction>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub strip_prefix: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub ssr_json: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub extract_pointer: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    #[serde(default = "default_param_type", rename = "type")]
    pub kind: String, // string|int|float|bool
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<Value>,
}

fn default_param_type() -> String {
    "string".into()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Body {
    #[serde(default, rename = "json")]
    pub json: std::collections::BTreeMap<String, Param>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_template: Option<Value>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub form: std::collections::BTreeMap<String, Param>,
}
impl Body {
    pub fn inputs(&self) -> impl Iterator<Item = (&String, &Param)> {
        self.json.iter().chain(self.form.iter())
    }
    pub fn content_type(&self) -> &'static str {
        if self.form.is_empty() {
            "application/json"
        } else {
            "application/x-www-form-urlencoded"
        }
    }
}

pub type Model = std::collections::BTreeMap<String, Field>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    #[serde(default)]
    pub path: String,
    #[serde(default, rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Health {
    pub op: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Issue {
    pub field: String,
    pub detail: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub remedy: String,
}

//! Provider adapters. API keys are transport-only and never become model input.
pub mod codex;
use crate::{
    apperr::{AppError, AppResult},
    util,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

#[derive(Clone, Serialize)]
pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    pub default_model: &'static str,
    pub key_url: &'static str,
}
pub const PROVIDERS: [Provider; 6] = [
    Provider {
        id: "openai",
        name: "OpenAI",
        default_model: "gpt-6.1-sol",
        key_url: "https://platform.openai.com/api-keys",
    },
    Provider {
        id: "anthropic",
        name: "Claude",
        default_model: "claude-opus-5-5",
        key_url: "https://platform.claude.com/settings/keys",
    },
    Provider {
        id: "xai",
        name: "xAI",
        default_model: "grok-4.7",
        key_url: "https://console.x.ai/",
    },
    Provider {
        id: "zai",
        name: "Z.ai",
        default_model: "glm-5.3",
        key_url: "https://z.ai/manage-apikey/apikey-list",
    },
    Provider {
        id: "zai-coding-plan",
        name: "Z.ai Coding Plan",
        default_model: "glm-5.3",
        key_url: "https://z.ai/manage-apikey/apikey-list",
    },
    Provider {
        id: "codex",
        name: "Codex",
        default_model: "",
        key_url: "",
    },
];
pub fn provider(id: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.id == id)
}
#[derive(Clone)]
pub struct Config {
    pub provider: String,
    pub model: String,
    pub key: String,
}
#[derive(Clone, Serialize)]
pub struct ProviderView {
    #[serde(flatten)]
    pub provider: Provider,
    pub model: String,
    pub connected: bool,
    pub has_key: bool,
    pub models: Vec<String>,
    pub available: bool,
    pub account: String,
}
pub struct Client {
    gate: tokio::sync::Semaphore,
    pub codex: Arc<codex::Codex>,
    pub origins: BTreeMap<String, String>,
}
impl Client {
    pub fn new(codex: Arc<codex::Codex>) -> Self {
        Self {
            gate: tokio::sync::Semaphore::new(3),
            codex,
            origins: BTreeMap::new(),
        }
    }
    pub async fn complete(&self, config: &Config, system: &str, prompt: &str) -> AppResult<String> {
        self.complete_when_ready(config, system, prompt, || Ok(()))
            .await
    }
    pub async fn complete_when_ready(
        &self,
        config: &Config,
        system: &str,
        prompt: &str,
        on_start: impl FnOnce() -> AppResult<()> + Send,
    ) -> AppResult<String> {
        let _permit = self
            .gate
            .acquire()
            .await
            .map_err(|_| AppError::api("cancelled", 409))?;
        on_start()?;
        let provider =
            provider(&config.provider).ok_or_else(|| AppError::api("provider_missing", 400))?;
        if config.provider == "codex" {
            return self.codex.complete(&config.model, system, prompt).await;
        }
        let model = if config.model.is_empty() {
            provider.default_model
        } else {
            &config.model
        };
        let (path, body) = match config.provider.as_str() {
            "anthropic" => (
                "/messages",
                json!({"model":model,"max_tokens":12000,"system":system,"messages":[{"role":"user","content":prompt}]}),
            ),
            "zai" | "zai-coding-plan" => (
                "/chat/completions",
                json!({"model":model,"max_tokens":16000,"messages":[{"role":"system","content":system},{"role":"user","content":prompt}]}),
            ),
            _ => (
                "/responses",
                json!({"model":model,"store":false,"max_output_tokens":16000,"input":[{"role":"system","content":system},{"role":"user","content":prompt}]}),
            ),
        };
        let value = self.request(config, "POST", path, Some(body)).await?;
        extract_text(&config.provider, &value)
    }
    pub async fn models(&self, config: &Config) -> AppResult<Vec<String>> {
        if config.provider == "codex" {
            return self.codex.models().await;
        }
        if is_zai(&config.provider) {
            self.complete(config, "Reply with OK only.", "Connection check.")
                .await?;
            return Ok(vec![if config.model.is_empty() {
                provider(&config.provider)
                    .ok_or_else(|| AppError::api("provider_missing", 400))?
                    .default_model
                    .into()
            } else {
                config.model.clone()
            }]);
        }
        let mut models = Vec::new();
        let mut path = "/models".to_string();
        for _ in 0..5 {
            let value = self.request(config, "GET", &path, None).await?;
            if let Some(data) = value.get("data").and_then(Value::as_array) {
                for model in data {
                    if let Some(id) = model.get("id").and_then(Value::as_str) {
                        models.push(id.into());
                    }
                }
            }
            if config.provider != "anthropic"
                || value.get("has_more").and_then(Value::as_bool) != Some(true)
            {
                break;
            }
            let Some(last) = value.get("last_id").and_then(Value::as_str) else {
                break;
            };
            path = format!(
                "/models?after_id={}",
                url::form_urlencoded::byte_serialize(last.as_bytes()).collect::<String>()
            );
        }
        models.sort();
        models.dedup();
        if models.is_empty() {
            return Err(AppError::api("model_unavailable", 400));
        }
        Ok(models)
    }
    pub async fn check(&self, config: &Config) -> AppResult<Vec<String>> {
        let models = self.models(config).await?;
        if !is_zai(&config.provider) {
            self.complete(config, "Reply with OK only.", "Connection check.")
                .await?;
        }
        Ok(models)
    }
    async fn request(
        &self,
        config: &Config,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> AppResult<Value> {
        if config.key.is_empty() {
            return Err(AppError::api("provider_missing", 400));
        }
        let default = api_origin(&config.provider)?;
        // Injectable only by local contract tests. There is no UI or model setting for this.
        let origin = self
            .origins
            .get(&config.provider)
            .map(String::as_str)
            .unwrap_or(default);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(240))
            .connect_timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .map_err(|_| AppError::api("ai_unavailable", 502))?;
        let method = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|_| AppError::api("internal", 500))?;
        let mut request = client
            .request(method, format!("{}{path}", origin.trim_end_matches('/')))
            .header("content-type", "application/json");
        if config.provider == "anthropic" {
            request = request
                .header("x-api-key", &config.key)
                .header("anthropic-version", "2023-06-01");
        } else {
            request = request.bearer_auth(&config.key);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.map_err(|e| {
            AppError::api(
                if e.is_timeout() {
                    "timeout"
                } else {
                    "ai_unavailable"
                },
                502,
            )
        })?;
        let status = response.status().as_u16();
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| AppError::api("ai_unavailable", 502))?;
            if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                return Err(AppError::api("ai_unavailable", 502));
            }
            bytes.extend_from_slice(&chunk);
        }
        if !(200..300).contains(&status) {
            return Err(provider_error(status, &bytes));
        }
        serde_json::from_slice(&bytes).map_err(|_| AppError::api("ai_unavailable", 502))
    }
}
fn is_zai(provider: &str) -> bool {
    matches!(provider, "zai" | "zai-coding-plan")
}
fn api_origin(provider: &str) -> AppResult<&'static str> {
    match provider {
        "openai" => Ok("https://api.openai.com/v1"),
        "anthropic" => Ok("https://api.anthropic.com/v1"),
        "xai" => Ok("https://api.x.ai/v1"),
        "zai" => Ok("https://api.z.ai/api/paas/v4"),
        "zai-coding-plan" => Ok("https://api.z.ai/api/coding/paas/v4"),
        _ => Err(AppError::api("provider_missing", 400)),
    }
}
pub fn extract_text(provider: &str, value: &Value) -> AppResult<String> {
    let mut text = String::new();
    match provider {
        "anthropic" => {
            if ![Some("end_turn"), Some("stop_sequence")]
                .contains(&value.get("stop_reason").and_then(Value::as_str))
                || value.get("error").is_some_and(|e| !e.is_null())
            {
                return Err(AppError::api("ai_unavailable", 502));
            }
            if let Some(content) = value.get("content").and_then(Value::as_array) {
                for block in content {
                    if block.get("type").and_then(Value::as_str) == Some("text") {
                        if let Some(s) = block.get("text").and_then(Value::as_str) {
                            text.push_str(s);
                        }
                    }
                }
            }
        }
        "zai" | "zai-coding-plan" => {
            if value
                .pointer("/choices/0/finish_reason")
                .and_then(Value::as_str)
                != Some("stop")
            {
                return Err(AppError::api("ai_unavailable", 502));
            }
            text = value
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into();
        }
        _ => {
            if value.get("status").and_then(Value::as_str) != Some("completed")
                || value.get("error").is_some_and(|e| !e.is_null())
            {
                return Err(AppError::api("ai_unavailable", 502));
            }
            if let Some(output) = value.get("output").and_then(Value::as_array) {
                for item in output {
                    if item.get("type").and_then(Value::as_str) == Some("message") {
                        if let Some(content) = item.get("content").and_then(Value::as_array) {
                            for block in content {
                                if block.get("type").and_then(Value::as_str) == Some("output_text")
                                {
                                    if let Some(s) = block.get("text").and_then(Value::as_str) {
                                        text.push_str(s);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if text.trim().is_empty() {
        return Err(AppError::api("ai_unavailable", 502));
    }
    Ok(util::redact_generated_text(&text))
}
pub fn provider_error(status: u16, bytes: &[u8]) -> AppError {
    let body = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    let code = if status == 401 || status == 403 {
        "credentials_invalid"
    } else if [
        "insufficient_quota",
        "insufficient balance",
        "1113",
        "credit balance",
    ]
    .iter()
    .any(|s| body.contains(s))
    {
        "insufficient_balance"
    } else if ["model_not_found", "model not found", "1311"]
        .iter()
        .any(|s| body.contains(s))
    {
        "model_unavailable"
    } else if status == 429 || status == 529 {
        "rate_limited"
    } else {
        "ai_unavailable"
    };
    AppError::api(code, if code == "rate_limited" { 429 } else { 502 })
}
pub fn decode_json<T: for<'de> Deserialize<'de>>(text: &str) -> AppResult<T> {
    let text = text.trim();
    let text = if text.starts_with("```") {
        text.split_once('\n')
            .map(|(_, body)| body.trim_end_matches('`').trim())
            .unwrap_or(text)
    } else {
        text
    };
    let start = text
        .find(['{', '['])
        .ok_or_else(|| AppError::api("ai_unavailable", 502))?;
    let mut de = serde_json::Deserializer::from_str(&text[start..]);
    T::deserialize(&mut de).map_err(|_| AppError::api("ai_unavailable", 502))
}

#[cfg(test)]
mod tests;

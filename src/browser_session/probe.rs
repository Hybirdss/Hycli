use chromiumoxide::Browser;
use futures_util::StreamExt;

use crate::{
    apperr::{self, AppError},
    defend,
};

use super::config::{STEALTH_INIT_SCRIPT, build_stealth_browser_config, can_run_headed};

#[derive(Debug, serde::Serialize)]
pub struct Observation {
    pub url: String,
    pub status: u16,
    pub defense: String,
    pub signal: String,
    pub latency_ms: i64,
    pub content_type: String,
}

async fn run_probe_with_mode(
    engine: &crate::environment::BrowserEngine,
    url: &str,
    headed: bool,
) -> Result<Observation, AppError> {
    let config = build_stealth_browser_config(engine, None, url, headed).map_err(|e| {
        apperr::runtime_msg(
            format!("Browser configuration failed: {e}"),
            "Install Chrome, Chromium, or Edge, or set CHROME to the executable path.",
        )
    })?;
    let (mut browser, mut handler) = Browser::launch(config).await.map_err(|e| {
        apperr::runtime(
            e,
            "Failed to launch Chromium",
            "Install Chrome, Chromium, or Edge, or set CHROME to the executable path.",
        )
    })?;
    let drive = tokio::spawn(async move { while let Some(_event) = handler.next().await {} });

    let outcome = observe(&browser, url).await;
    let _ = browser.close().await;
    let _ = drive.abort();
    outcome
}

pub async fn stealth_probe(url: &str) -> Result<Observation, AppError> {
    let environment = tokio::task::spawn_blocking(crate::environment::inspect)
        .await
        .map_err(|_| AppError::api("browser_unavailable", 422))?;
    let engine = environment
        .browsers
        .iter()
        .find(|b| b.protocol == "chromium-cdp")
        .ok_or_else(|| AppError::api("browser_unavailable", 422))?;
    let force_headed = std::env::var_os("HYCLI_HEADED").is_some();
    let first = run_probe_with_mode(engine, url, force_headed).await?;
    if !force_headed && defend::is_challenge_label(&first.defense) && can_run_headed() {
        if let Ok(escalated) = run_probe_with_mode(engine, url, true).await {
            if !defend::is_challenge_label(&escalated.defense) {
                return Ok(escalated);
            }
        }
    }
    Ok(first)
}

async fn observe(browser: &Browser, url: &str) -> Result<Observation, AppError> {
    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| apperr::runtime(e, "Failed to open browser page", ""))?;
    let _ = page.evaluate_on_new_document(STEALTH_INIT_SCRIPT).await;

    let start = std::time::Instant::now();
    let handle = page
        .goto(url)
        .await
        .map_err(|e| apperr::runtime(e, format!("Navigation failed: {url}"), "Check the URL."))?;
    let http_req = handle
        .wait_for_navigation_response()
        .await
        .map_err(|e| apperr::runtime(e, "Failed waiting for navigation response", ""))?
        .ok_or_else(|| {
            apperr::runtime_msg(
                "No navigation response received",
                "Check network connectivity and the URL.",
            )
        })?;
    let latency = start.elapsed().as_millis() as i64;
    let html = page.content().await.unwrap_or_default();

    let status = http_req
        .response
        .as_ref()
        .map(|r| r.status as u16)
        .unwrap_or(0);
    let mut headers: Vec<(String, String)> = Vec::new();
    for (k, v) in &http_req.headers {
        headers.push((k.to_lowercase(), v.clone()));
    }
    let content_type = headers
        .iter()
        .find(|(k, _)| k == "content-type")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();

    let (cls, sig) = defend::classify(&defend::Observation {
        status_code: status,
        headers: headers.clone(),
        body_snippet: html.chars().take(4096).collect(),
    });
    Ok(Observation {
        url: url.into(),
        status,
        defense: cls.as_str().into(),
        signal: sig.into(),
        latency_ms: latency,
        content_type,
    })
}

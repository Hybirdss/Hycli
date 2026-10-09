//! Browser rendering with a discovered engine and its normal platform defaults.
use chromiumoxide::{Browser, browser::BrowserConfig};
use futures_util::StreamExt;

use crate::apperr::{self, AppError};
use crate::defend;

#[derive(Debug, serde::Serialize)]
pub struct Observation {
    pub url: String,
    pub status: u16,
    pub defense: String,
    pub signal: String,
    pub latency_ms: i64,
    pub content_type: String,
}

/// Legacy entry point retained for CLI compatibility; no fingerprint or language overrides.
pub async fn stealth_probe(url: &str) -> Result<Observation, AppError> {
    let environment = tokio::task::spawn_blocking(crate::environment::inspect)
        .await
        .map_err(|_| AppError::api("browser_unavailable", 422))?;
    let engine = environment
        .browsers
        .iter()
        .find(|b| b.protocol == "chromium-cdp")
        .ok_or_else(|| AppError::api("browser_unavailable", 422))?;
    let config = BrowserConfig::builder()
        .chrome_executable(&engine.executable)
        .window_size(1280, 800)
        .arg("--no-first-run")
        .build()
        .map_err(|e| {
            apperr::runtime_msg(
                format!("브라우저 구성 실패: {e}"),
                "Chrome/Chromium/Edge를 설치하거나 CHROME 환경변수에 실행 파일 경로를 지정하세요",
            )
        })?;
    let (mut browser, mut handler) = Browser::launch(config).await.map_err(|e| {
        apperr::runtime(
            e,
            "크로뮴 실행 실패",
            "Chrome/Chromium/Edge를 설치하거나 CHROME 환경변수에 실행 파일 경로를 지정하세요",
        )
    })?;
    let drive = tokio::spawn(async move { while let Some(_event) = handler.next().await {} });

    let outcome = observe(&browser, url).await;
    let _ = browser.close().await;
    let _ = drive.abort();
    outcome
}

async fn observe(browser: &Browser, url: &str) -> Result<Observation, AppError> {
    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| apperr::runtime(e, "페이지 생성 실패", ""))?;

    let start = std::time::Instant::now();
    let handle = page
        .goto(url)
        .await
        .map_err(|e| apperr::runtime(e, format!("탐색 실패: {url}"), "URL 확인"))?;
    let http_req = handle
        .wait_for_navigation_response()
        .await
        .map_err(|e| apperr::runtime(e, "응답 대기 실패", ""))?
        .ok_or_else(|| apperr::runtime_msg("본 응답 미수신", "네트워크/URL 재확인"))?;
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

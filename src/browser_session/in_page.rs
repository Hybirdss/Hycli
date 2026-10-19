use crate::{
    accounts::Credential,
    apperr::{AppError, AppResult},
};
use chromiumoxide::Browser;
use futures_util::StreamExt;
use serde_json::Value;
use std::time::Duration;

use super::{
    config::{
        STEALTH_INIT_SCRIPT, build_stealth_browser_config, can_run_headed, cookie_param_for,
        unavailable,
    },
    profile_lock::{acquire_profile_dir, discover_live_cdp_endpoint},
};

pub struct InPageFetchRequest<'a> {
    pub credential: &'a Credential,
    pub method: &'a str,
    pub url: &'a str,
    pub headers: &'a [(String, String)],
    pub body: Option<&'a [u8]>,
    pub allow_local: bool,
}

pub struct InPageFetchResponse {
    pub url: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub bytes: Vec<u8>,
}

async fn run_fetch_in_page_with_mode(
    req: &InPageFetchRequest<'_>,
    origin_str: &str,
    engine: &crate::environment::BrowserEngine,
    headed: bool,
) -> AppResult<InPageFetchResponse> {
    let (profile_dir, _profile_lock, _snapshot) =
        acquire_profile_dir(origin_str).map_err(unavailable)?;
    let config = build_stealth_browser_config(engine, Some(&profile_dir), origin_str, headed)
        .map_err(unavailable)?;
    let (mut browser, mut handler) =
        tokio::time::timeout(Duration::from_secs(30), Browser::launch(config))
            .await
            .map_err(unavailable)?
            .map_err(unavailable)?;
    let drive = tokio::spawn(async move { while handler.next().await.is_some() {} });
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        execute_in_page_fetch(&browser, req, origin_str),
    )
    .await
    .map_err(unavailable)
    .and_then(|r| r);
    let _ = browser.close().await;
    drive.abort();
    result
}

pub async fn fetch_in_page(req: InPageFetchRequest<'_>) -> AppResult<InPageFetchResponse> {
    let target = crate::net::validate_url(req.url)?;
    crate::net::resolve(&target, req.allow_local).await?;
    let origin_str = if req.credential.origin.is_empty() {
        target.origin().ascii_serialization()
    } else {
        let cred_origin = crate::net::validate_url(&req.credential.origin)?;
        if !crate::net::same_site_origin(&target, &cred_origin) {
            return Err(AppError::api("unsafe_target", 400));
        }
        cred_origin.origin().ascii_serialization()
    };

    if let Some(cdp_url) = discover_live_cdp_endpoint(req.credential) {
        if let Ok(Ok((browser, mut handler))) =
            tokio::time::timeout(Duration::from_secs(3), Browser::connect(cdp_url)).await
        {
            let drive = tokio::spawn(async move { while handler.next().await.is_some() {} });
            let res = tokio::time::timeout(
                Duration::from_secs(30),
                execute_in_page_fetch(&browser, &req, &origin_str),
            )
            .await
            .map_err(unavailable)
            .and_then(|r| r);
            drive.abort();
            if res.is_ok() {
                return res;
            }
        }
    }

    let environment = tokio::task::spawn_blocking(crate::environment::inspect)
        .await
        .map_err(unavailable)?;
    let engine = environment
        .browsers
        .iter()
        .find(|b| b.protocol == "chromium-cdp")
        .ok_or_else(|| AppError::api("browser_unavailable", 422))?;
    let force_headed = std::env::var_os("HYCLI_HEADED").is_some();
    let first = run_fetch_in_page_with_mode(&req, &origin_str, engine, force_headed).await?;
    let (cls, _) = crate::defend::classify(&crate::defend::Observation {
        status_code: first.status,
        headers: first.headers.clone(),
        body_snippet: String::from_utf8_lossy(&first.bytes)
            .chars()
            .take(4096)
            .collect(),
    });
    if !force_headed && cls.is_challenge() && can_run_headed() {
        if let Ok(escalated) = run_fetch_in_page_with_mode(&req, &origin_str, engine, true).await {
            return Ok(escalated);
        }
    }
    Ok(first)
}

async fn execute_in_page_fetch(
    browser: &Browser,
    req: &InPageFetchRequest<'_>,
    origin_str: &str,
) -> AppResult<InPageFetchResponse> {
    let page = browser.new_page("about:blank").await.map_err(unavailable)?;
    for cookie in &req.credential.cookies {
        let value = cookie_param_for(cookie, origin_str);
        page.set_cookie(value).await.map_err(unavailable)?;
    }
    let origin_json = serde_json::to_string(origin_str)?;
    let storage_json = serde_json::to_string(&req.credential.local_storage)?;
    page.evaluate_on_new_document(format!(
        "{STEALTH_INIT_SCRIPT}\nif(location.origin==={origin_json}){{for(const [key,value] of Object.entries({storage_json})){{try{{localStorage.setItem(key,value);sessionStorage.setItem(key,value)}}catch(_){{}}}}}}"
    ))
    .await
    .map_err(unavailable)?;

    let bootstrap_url = format!("{origin_str}/");
    let _ = tokio::time::timeout(Duration::from_secs(12), page.goto(&bootstrap_url)).await;
    for _ in 0..10 {
        if let Ok(dom) = page.content().await {
            let lower = dom.to_ascii_lowercase();
            if !lower.contains("just a moment")
                && !lower.contains("cf-chl")
                && !lower.contains("challenge-platform")
            {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    use base64::Engine;
    let body_b64 = req
        .body
        .map(|b| base64::engine::general_purpose::STANDARD.encode(b));
    let cfg = serde_json::json!({
        "url": req.url,
        "method": req.method.to_ascii_uppercase(),
        "headers": req.headers,
        "bodyBase64": body_b64,
    });
    let script = format!(
        r#"(async () => {{
  const cfg = {cfg};
  const headers = {{}};
  for (const [k, v] of cfg.headers) {{
    const lower = String(k).toLowerCase();
    if (!['cookie', 'cookie2', 'host', 'origin', 'referer', 'user-agent', 'content-length', 'transfer-encoding', 'connection'].includes(lower) && !lower.startsWith('sec-') && !lower.startsWith('proxy-')) {{
      headers[k] = v;
    }}
  }}
  const init = {{
    method: cfg.method,
    headers,
    credentials: 'include',
    mode: 'cors',
    redirect: 'follow'
  }};
  if (cfg.bodyBase64) {{
    const bin = atob(cfg.bodyBase64);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    init.body = bytes;
  }}
  const resp = await fetch(cfg.url, init);
  const buf = await resp.arrayBuffer();
  if (buf.byteLength > 8 * 1024 * 1024) {{
    throw new Error('too_large');
  }}
  const bytes = new Uint8Array(buf);
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {{
    binary += String.fromCharCode.apply(null, bytes.subarray(i, i + chunk));
  }}
  const outHeaders = [];
  resp.headers.forEach((v, k) => outHeaders.push([String(k).toLowerCase(), String(v)]));
  return {{
    status: resp.status,
    url: resp.url || cfg.url,
    headers: outHeaders,
    bodyBase64: btoa(binary)
  }};
}})()"#
    );

    let isolated_ctx = if let Ok(Some(frame_id)) = page.mainframe().await {
        page.execute(
            chromiumoxide::cdp::browser_protocol::page::CreateIsolatedWorldParams::builder()
                .frame_id(frame_id)
                .world_name("hycli_isolated_fetch")
                .grant_univeral_access(true)
                .build()
                .map_err(unavailable)?,
        )
        .await
        .ok()
        .map(|r| r.result.execution_context_id)
    } else {
        None
    };

    let mut eval_builder = chromiumoxide::cdp::js_protocol::runtime::EvaluateParams::builder()
        .expression(script)
        .await_promise(true)
        .return_by_value(true);
    if let Some(ctx_id) = isolated_ctx {
        eval_builder = eval_builder.context_id(ctx_id);
    }
    let eval_params = eval_builder.build().map_err(unavailable)?;
    let eval_res = page.evaluate_expression(eval_params).await;
    let _ = page.close().await;
    let val: Value = eval_res
        .map_err(unavailable)?
        .into_value()
        .map_err(unavailable)?;
    let status = val["status"].as_u64().unwrap_or(0) as u16;
    let final_url = val["url"].as_str().unwrap_or(req.url).to_string();
    let headers: Vec<(String, String)> = val["headers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|pair| {
            let arr = pair.as_array()?;
            Some((
                arr.first()?.as_str()?.to_string(),
                arr.get(1)?.as_str()?.to_string(),
            ))
        })
        .collect();
    let b64 = val["bodyBase64"].as_str().unwrap_or("");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(unavailable)?;
    Ok(InPageFetchResponse {
        url: final_url,
        status,
        headers,
        bytes,
    })
}

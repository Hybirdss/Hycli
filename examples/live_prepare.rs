//! Opt-in smoke test with the user's managed Codex connection and a public website.
//! Pass a website URL explicitly. Never executes a mutation or prints credentials/results.
use hycli::{
    runtime::Runtime,
    state::{Job, ProviderSettings},
    util,
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Duration};
async fn wait(core: &Arc<Runtime>, job: &Job) -> Result<Job, Box<dyn std::error::Error>> {
    let started = std::time::Instant::now();
    let mut prior = String::new();
    loop {
        let current = core.state.job(&job.id)?;
        let stage = format!("{}:{}", current.kind, current.phase);
        if stage != prior {
            println!("{stage}");
            prior = stage;
        }
        if !["queued", "running"].contains(&current.status.as_str()) {
            if current.status != "completed" {
                return Err(format!("{} failed: {}", current.kind, current.error_code).into());
            }
            return Ok(current);
        }
        if started.elapsed() > Duration::from_secs(900) {
            core.cancel_job(&job.id)?;
            return Err("Smoke test exceeded 15 minutes".into());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::args()
        .nth(1)
        .ok_or("Usage: cargo run --example live_prepare -- https://your-public-website.example")?;
    let root = std::env::var_os("HYCLI_TEST_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/live-prepare"))
        .join(util::id());
    let core = Runtime::open(&root, None, None, true, false, false)?;
    if core.ai.codex.refresh_account().await?.is_none() {
        return Err("A managed ChatGPT sign-in is required".into());
    }
    let models = core.ai.codex.models().await?;
    core.state.update(|data| {
        data.settings.default_provider = "codex".into();
        data.providers.insert(
            "codex".into(),
            ProviderSettings {
                model: std::env::var("HYCLI_TEST_MODEL").unwrap_or_else(|_| "gpt-6-sol".into()),
                connected: true,
                models,
                checked_at: util::now(),
                ..Default::default()
            },
        );
        Ok(())
    })?;
    println!("Managed connection ready; preparing the supplied public website.");
    let intent = std::env::args().nth(2).unwrap_or_default();
    let job = core.start_prepare_with_intent(&url, "", "codex", "en", &intent)?;
    let prepared = wait(&core, &job).await?;
    let sp = core.spec(&prepared.site_id)?;
    let operation = sp
        .operations
        .iter()
        .filter(|op| {
            hycli::policy::operation_effect(op, &BTreeMap::new()) == hycli::policy::Effect::Read
                && hycli::runtime::params(op).iter().all(|(_, p)| !p.required)
                && sp
                    .find_auth(&op.auth)
                    .is_none_or(|auth| auth.kind == "none")
        })
        .next()
        .ok_or("No public read with complete default inputs was prepared")?;
    let request = core.request(&sp, &operation.name, BTreeMap::new(), Some(""))?;
    let action = core.start_action(request, None, "en")?;
    let completed = wait(&core, &action).await?;
    let summary = core.start_summary(&action.id, "codex")?;
    wait(&core, &summary).await?;
    println!(
        "{}",
        serde_json::json!({"live_service":"passed","workers":prepared.agents.len(),"actions":sp.operations.len(),"read_completed":completed.result.is_some(),"summary_saved":!core.state.job(&action.id)?.summary.is_empty(),"data":root})
    );
    Ok(())
}

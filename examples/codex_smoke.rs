use hycli::ai::codex::Codex;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cwd = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/codex-smoke");
    let codex = Codex::new(cwd);
    if !codex.available() {
        println!("Codex unavailable");
        return Ok(());
    }
    let connected = codex.refresh_account().await?.is_some();
    println!("Managed ChatGPT sign-in connected: {connected}");
    if !connected {
        return Ok(());
    }
    let models = codex.models().await?;
    println!("Available models: {}", models.len());
    let answer = codex
        .complete(
            "",
            "Return exactly OK. Do not call tools, read files, use apps, or access websites.",
            "This is a text-only adapter connection check. Return OK.",
        )
        .await?;
    println!("Text-only inference check: {}", answer.trim() == "OK");
    Ok(())
}

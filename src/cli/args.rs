use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "hycli", version, about = "Automate any website.")]
pub struct Cli {
    #[arg(long, global = true)]
    pub kb: Option<String>,
    #[arg(long, global = true)]
    pub site: Option<String>,
    #[arg(long, global = true, default_value = "auto")]
    pub format: String,
    /// Permit requests to explicitly trusted private/local websites.
    #[arg(long, global = true)]
    pub allow_local: bool,
    #[command(subcommand)]
    pub cmd: Option<Cmd>,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Inspect a website with one bounded read.
    Probe {
        url: String,
        #[arg(long, default_value = "go")]
        tls_profile: String,
        /// Observe using a local Chromium browser instance.
        #[arg(long)]
        browser: Option<String>,
        /// CDP attach: auto | ws://… | http://127.0.0.1:9222
        #[arg(long)]
        cdp: Option<String>,
    },
    /// Deterministic fast recon: mine SSR state, documented routes, and JS bundle APIs without an AI loop.
    Recon {
        url: String,
        /// Write a draft SiteSpec v2 YAML file to this path.
        #[arg(long)]
        draft: Option<String>,
        /// Maximum number of JS bundles to scan for API routes.
        #[arg(long, default_value_t = 12)]
        max_bundles: usize,
    },
    /// Inspect the local evidence and activity database.
    Kb {
        #[command(subcommand)]
        cmd: KbCmd,
    },
    /// Install, validate and manage website definitions.
    Spec {
        #[command(subcommand)]
        cmd: SpecCmd,
    },
    /// Manage credentials in the local vault.
    Auth {
        #[command(subcommand)]
        cmd: AuthCmd,
    },
    /// Convert a HAR recording into a draft website definition.
    Capture {
        #[arg(long)]
        from: String,
        #[arg(long)]
        site: String,
        #[arg(long)]
        base: String,
        #[arg(long)]
        out: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Run an installed action and include response metadata.
    Try {
        site: String,
        op: String,
        /// Pass --arg key=value (may be repeated).
        #[arg(long = "arg")]
        args: Vec<String>,
    },
    /// Replay recorded reads without AI: tells an expired sign-in from a changed website.
    #[command(visible_alias = "health")]
    Check {
        /// Installed website ID; omit to check every website.
        site: Option<String>,
    },
    /// Describe websites, actions, typed inputs and expected results as JSON.
    Describe {
        site: Option<String>,
        action: Option<String>,
    },
    /// Prepare a website with your connected AI provider and wait for the result.
    Prepare {
        url: String,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        language: Option<String>,
        /// Describe the outcome and related capabilities you want the AI to prepare.
        #[arg(long)]
        intent: Option<String>,
    },
    /// Ask the AI to add capabilities to an installed website, keeping existing actions.
    #[command(visible_alias = "extend")]
    Request {
        site: String,
        /// Describe the missing functionality in your own words.
        instruction: String,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        language: Option<String>,
    },
    /// Run an action with exact input names and an optional saved account.
    Run {
        site: String,
        action: String,
        #[arg(long = "arg", value_name = "NAME=VALUE")]
        args: Vec<String>,
        #[arg(long)]
        account: Option<String>,
    },
    /// Check or set whether write actions are allowed for an installed website CLI.
    Writes {
        site: String,
        /// Pass `allow` (`on` / `true`) after asking the user once for this CLI, or `deny` (`off` / `false`) to disable. Omit to inspect status.
        mode: Option<String>,
    },
    /// List saved website accounts without exposing sign-in secrets.
    Accounts {
        #[arg(long)]
        site: Option<String>,
        #[command(subcommand)]
        cmd: Option<AccountsCmd>,
    },
    /// Discover existing browser profiles. Never returns sign-in values.
    Browsers {
        /// Find existing sign-ins for this website inside the local broker.
        #[arg(long)]
        url: Option<String>,
    },
    /// Read work progress and plain-language notes. Credentials are never included.
    Jobs {
        #[command(subcommand)]
        cmd: JobsCmd,
    },
    /// Print the installed version.
    Version,
    /// Request a browser to open a pre-filled GitHub issue with your version and platform. No issue is submitted automatically.
    Feedback,
    /// Installed website commands: hycli <site> <action> --input value.
    #[clap(external_subcommand)]
    Site(Vec<String>),
    /// Serve installed website tools over MCP (stdio).
    Mcp {
        /// Expose only installed website actions.
        #[arg(long)]
        sites_only: bool,
        /// Limit the tool surface to one installed website.
        #[arg(long)]
        only_site: Option<String>,
    },
    /// Open Hycli, starting its background engine only when needed.
    Open {
        #[arg(long, default_value_t = 4318)]
        port: u16,
        #[arg(long)]
        no_open: bool,
    },
    /// Show whether the local dashboard is running.
    Status,
    /// Stop the local dashboard after its active work has finished.
    Stop,
    /// Run the local dashboard in the foreground (for development or servers).
    Dashboard {
        #[arg(long, default_value_t = 4318)]
        port: u16,
        #[arg(long)]
        no_open: bool,
        #[arg(long, hide = true)]
        auto_port: bool,
    },
}

#[derive(Subcommand)]
pub enum JobsCmd {
    /// List recent work, stages and agent status.
    Ls,
    /// Read one work log. --watch follows it until completion.
    Show {
        id: String,
        #[arg(long)]
        watch: bool,
    },
}

#[derive(Subcommand)]
pub enum KbCmd {
    Init,
    Next,
    Gaps {
        #[arg(long, default_value = "open")]
        state: String,
    },
    GapSet {
        id: i64,
        state: String,
    },
    NodeLs {
        kind: String,
        #[arg(long, default_value_t = 200)]
        limit: i64,
    },
    NodeGet {
        kind: String,
        key: String,
    },
    Graph {
        node_id: i64,
        #[arg(long, default_value_t = 2)]
        depth: i64,
    },
    Record {
        #[arg(long)]
        action: String,
        #[arg(long)]
        verdict: String,
        #[arg(long, default_value = "access")]
        stage: String,
        #[arg(long, default_value = "")]
        target_key: String,
        #[arg(long, default_value_t = 0)]
        status: i64,
        #[arg(long, default_value = "")]
        defense: String,
    },
    FindingAdd {
        statement: String,
        #[arg(long, default_value_t = 0.8)]
        confidence: f64,
        #[arg(long, default_value = "")]
        evidence: String,
    },
    FindingSearch {
        #[arg(default_value = "")]
        q: String,
    },
    Stats,
    Fsck,
}

#[derive(Subcommand)]
pub enum SpecCmd {
    Validate { file: String },
    Install { file: String },
    Ls,
    Rm { site: String },
}

#[derive(Subcommand)]
pub enum AuthCmd {
    /// Store a legacy API key or an explicitly bound OAuth grant. OAuth JSON must use stdin.
    Set {
        r#ref: String,
        /// Legacy API-key value only. Send OAuth grant JSON on stdin instead.
        #[arg(long)]
        value: Option<String>,
    },
    Ls,
    Rm {
        r#ref: String,
    },
}

#[derive(Subcommand)]
pub enum AccountsCmd {
    /// Read one evidenced JSON page with a verified account, without installing a website action.
    Read {
        account: String,
        /// Version 1 request/evidence JSON. Never include credentials.
        #[arg(long)]
        request: String,
        /// Opaque next_cursor from the same account and unchanged request file.
        #[arg(long)]
        cursor: Option<String>,
    },
    /// Inspect cookie/header names and storage structure, never sign-in values.
    Inspect { account: String },
    /// Observe one same-site GET before identity verification; private values stay local.
    Observe {
        account: String,
        url: String,
        /// Reserved browser mode; currently fails closed. Use a same-origin HTTP endpoint.
        #[arg(long)]
        browser: bool,
    },
    /// Verify an observed current-account GET against an anonymous baseline.
    Verify {
        account: String,
        /// JSON connection recipe containing local key references, never secret values.
        #[arg(long)]
        recipe: String,
    },
    /// Recheck a saved account using its verified connection recipe.
    Refresh { account: String },
}

pub struct Deps {
    pub kb: Option<String>,
    pub site: String,
    pub force_json: bool,
    pub allow_local: bool,
}

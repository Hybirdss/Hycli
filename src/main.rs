use clap::{Parser, Subcommand};

use hycli::apperr::{self, AppError};
use hycli::output::Emitter;

#[derive(Parser)]
#[command(name = "hycli", version, about = "Websites, ready for AI.")]
struct Cli {
    #[arg(long, global = true)]
    kb: Option<String>,
    #[arg(long, global = true)]
    site: Option<String>,
    #[arg(long, global = true, default_value = "auto")]
    format: String,
    /// Permit requests to explicitly trusted private/local websites.
    #[arg(long, global = true)]
    allow_local: bool,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Inspect a website with one bounded read.
    Probe {
        url: String,
        #[arg(long, default_value = "go")]
        tls_profile: String,
        /// 브라우저 관찰 — "stealth"=런치(예정), 실무는 --cdp
        #[arg(long)]
        browser: Option<String>,
        /// CDP attach: auto | ws://… | http://127.0.0.1:9222
        #[arg(long)]
        cdp: Option<String>,
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
        /// --arg k=v 반복
        #[arg(long = "arg")]
        args: Vec<String>,
    },
    /// Check a website using an available read that requires no inputs.
    Health { site: String },
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
    /// List saved website accounts without exposing sign-in secrets.
    Accounts {
        #[arg(long)]
        site: Option<String>,
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
enum JobsCmd {
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
enum KbCmd {
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
enum SpecCmd {
    Validate { file: String },
    Install { file: String },
    Ls,
    Rm { site: String },
}

#[derive(Subcommand)]
enum AuthCmd {
    Set {
        r#ref: String,
        #[arg(long)]
        value: Option<String>,
    },
    Ls,
    Rm {
        r#ref: String,
    },
}

struct Deps {
    kb: Option<String>,
    site: String,
    force_json: bool,
    allow_local: bool,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let deps = Deps {
        kb: cli.kb,
        site: cli.site.unwrap_or_else(|| "default".into()),
        force_json: cli.format == "json",
        allow_local: cli.allow_local,
    };
    let emit = Emitter::new(deps.force_json);
    let code = run(
        deps,
        cli.cmd.unwrap_or(Cmd::Open {
            port: 4318,
            no_open: false,
        }),
        &emit,
    )
    .await;
    std::process::exit(code);
}

async fn run(deps: Deps, cmd: Cmd, emit: &Emitter) -> i32 {
    match dispatch(deps, cmd, emit).await {
        Ok(()) => 0,
        Err(e) => emit.fail(&e),
    }
}

fn runner(deps: &Deps) -> Result<std::sync::Arc<hycli::runtime::Runtime>, AppError> {
    runtime(deps, false, deps.allow_local)
}
fn runtime(
    deps: &Deps,
    recover: bool,
    allow_local: bool,
) -> Result<std::sync::Arc<hycli::runtime::Runtime>, AppError> {
    hycli::runtime::Runtime::open(
        &hycli::platform::data_dir(),
        Some(hycli::platform::specs_dir()),
        Some(hycli::platform::resolve_kb(deps.kb.as_deref())),
        recover,
        allow_local,
        true,
    )
}

fn open_kb(deps: &Deps) -> Result<std::sync::Arc<hycli::kb::Kb>, AppError> {
    Ok(std::sync::Arc::new(hycli::kb::Kb::open(
        &hycli::platform::resolve_kb(deps.kb.as_deref()),
    )?))
}

fn find_spec(name: &str) -> Result<hycli::spec::Spec, AppError> {
    if !hycli::util::valid_id(name) {
        return Err(apperr::usage(
            "Invalid website ID",
            "Use `hycli spec ls` to list installed websites.",
        ));
    }
    let path = hycli::platform::specs_dir().join(format!("{name}.yaml"));
    if !path.exists() {
        return Err(apperr::spec(
            format!("Website is not installed: {name}"),
            "Use `hycli prepare URL`, open `hycli open`, or install a definition with `hycli spec install FILE`.",
        ));
    }
    let spec = hycli::spec::parse(&hycli::util::read_bounded(&path, 2 * 1024 * 1024)?)?;
    hycli::policy::validate_spec(&spec).map_err(|_| {
        apperr::spec(
            format!("Invalid installed website definition: {name}"),
            format!(
                "Run `hycli spec validate {}` or prepare the website again.",
                path.display()
            ),
        )
    })?;
    if spec.site.name != name {
        return Err(apperr::spec(
            "Website ID does not match its filename",
            "Reinstall the website definition.",
        ));
    }
    Ok(spec)
}

fn hycli_sites() -> Vec<(String, hycli::spec::Spec)> {
    let dir = hycli::platform::specs_dir();
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut paths: Vec<_> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            if p.extension().map(|e| e == "yaml").unwrap_or(false) {
                if let Some(id) = p.file_stem().and_then(|s| s.to_str()) {
                    match find_spec(id) {
                        Ok(sp) => out.push((sp.site.name.clone(), sp)),
                        Err(error) => eprintln!("hycli: cannot load {id}: {error}"),
                    }
                }
            }
        }
    }
    out
}

fn load_file_spec(file: &str) -> Result<hycli::spec::Spec, AppError> {
    let data = std::fs::read(file)
        .map_err(|e| apperr::spec(format!("파일 읽기 실패 ({e})"), "경로 확인"))?;
    hycli::spec::parse(&data)
}

fn blocked_err(defense: &str, signal: &str) -> AppError {
    let cls = match defense {
        "cloudflare" => hycli::defend::Class::Cloudflare,
        "bot-manager" => hycli::defend::Class::BotManager,
        "rate-limit" => hycli::defend::Class::RateLimit,
        "captcha" => hycli::defend::Class::Captcha,
        _ => hycli::defend::Class::Unknown,
    };
    let sig = if signal.is_empty() {
        String::new()
    } else {
        format!(" ({signal})")
    };
    apperr::blocked(format!("차단 감지: {defense}{sig}"), cls.remedy())
}

async fn dispatch(deps: Deps, cmd: Cmd, emit: &Emitter) -> Result<(), AppError> {
    match cmd {
        Cmd::Browsers { url } => {
            if let Some(url) = url {
                emit.data(&runner(&deps)?.find_browser_accounts("", &url, "").await?);
            } else {
                emit.data(&hycli::browser_discovery::discover());
            }
            Ok(())
        }
        Cmd::Jobs { cmd } => {
            let core = runner(&deps)?;
            match cmd {
                JobsCmd::Ls => {
                    let jobs: Vec<_> = core
                        .state
                        .read()?
                        .jobs
                        .into_iter()
                        .map(|mut job| {
                            job.result = None;

                            job.url.clear();
                            job
                        })
                        .collect();
                    emit.data(&jobs);
                }
                JobsCmd::Show { id, watch } => {
                    let mut previous = String::new();
                    loop {
                        let mut job = core.state.job(&id)?;

                        job.url.clear();
                        if job.updated_at != previous {
                            emit.data(&job);
                            previous = job.updated_at.clone();
                        }
                        if !watch || !["running", "queued"].contains(&job.status.as_str()) {
                            job_outcome(&job)?;
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(750)).await;
                    }
                }
            }
            Ok(())
        }
        Cmd::Version => {
            emit.data(&serde_json::json!({"version": env!("CARGO_PKG_VERSION")}));
            Ok(())
        }
        Cmd::Describe { site, action } => {
            // Diagnose damaged definitions instead of silently losing them from discovery.
            let _ = hycli_sites();
            if let Some(id) = &site {
                find_spec(id)?;
            }
            let core = runner(&deps)?;
            let sites = core.sites(&core.state.read()?.settings.locale)?;
            if let Some(id) = site {
                let site = sites
                    .into_iter()
                    .find(|s| s.id == id)
                    .ok_or_else(|| AppError::api("not_found", 404))?;
                if let Some(id) = action {
                    let action = site
                        .actions
                        .into_iter()
                        .find(|a| a.id == id)
                        .ok_or_else(|| AppError::api("not_found", 404))?;
                    emit.data(&action);
                } else {
                    emit.data(&site);
                }
                return Ok(());
            }
            emit.data(&serde_json::json!({
                "name": "hycli", "version": env!("CARGO_PKG_VERSION"), "site": deps.site, "sites": sites,
                "contract": {
                    "stdout": "JSON results; progress and errors use stderr",
                    "stderr": "hycli: code=N message — remedy: next step",
                    "exit_codes": {"ok": 0, "runtime": 1, "usage": 2, "auth": 3, "blocked": 4, "spec": 5},
                },
                "usage": "hycli SITE ACTION --input value; hycli SITE ACTION --help",
            }));
            Ok(())
        }
        Cmd::Accounts { site } => {
            let accounts: Vec<_> = runner(&deps)?
                .state
                .read()?
                .accounts
                .into_values()
                .filter(|a| site.as_ref().is_none_or(|id| a.site_id == *id))
                .collect();
            emit.data(&accounts);
            Ok(())
        }
        Cmd::Prepare {
            url,
            provider,
            language,
            intent,
        } => {
            let core = runner(&deps)?;
            let locale = language.unwrap_or(core.state.read()?.settings.locale);
            let job = core.start_prepare_with_intent(
                &url,
                "",
                provider.as_deref().unwrap_or(""),
                &locale,
                intent.as_deref().unwrap_or(""),
            )?;
            wait_preparation(&core, &job.id, emit).await
        }
        Cmd::Request {
            site,
            instruction,
            provider,
            language,
        } => {
            let core = runner(&deps)?;
            let locale = language.unwrap_or(core.state.read()?.settings.locale);
            let job = core.start_request(
                &site,
                &instruction,
                provider.as_deref().unwrap_or(""),
                &locale,
            )?;
            wait_preparation(&core, &job.id, emit).await
        }
        Cmd::Run {
            site,
            action,
            args,
            account,
        } => {
            let sp = find_spec(&site)?;
            let args = hycli::cli_actions::pairs(&args)?;
            let (result, _) = runner(&deps)?
                .run_op_for_account(&sp, &action, &args, account.as_deref())
                .await?;
            emit.data(&result.mapped.or(result.body));
            Ok(())
        }
        Cmd::Open { port, no_open } => {
            let instance =
                hycli::desktop::open(port, !no_open, deps.allow_local, deps.kb.as_deref()).await?;
            emit.data(&serde_json::json!({"running": true, "url": instance.url()}));
            Ok(())
        }
        Cmd::Status => {
            let instance = hycli::desktop::running(&hycli::platform::data_dir()).await;
            emit.data(&serde_json::json!({"running": instance.is_some(), "url": instance.map(|i| i.url())}));
            Ok(())
        }
        Cmd::Stop => {
            hycli::desktop::stop(&hycli::platform::data_dir()).await?;
            emit.data(&serde_json::json!({"running": false}));
            Ok(())
        }
        Cmd::Dashboard {
            port,
            no_open,
            auto_port,
        } => {
            hycli::dashboard::serve_with_options(
                runtime(&deps, true, deps.allow_local)?,
                port,
                !no_open,
                auto_port,
            )
            .await
        }
        Cmd::Mcp {
            sites_only,
            only_site,
        } => {
            let core = runner(&deps)?;
            if let Some(site) = &only_site {
                core.spec(site)?;
            }
            hycli::guarded_mcp::serve(hycli::guarded_mcp::Server {
                core,
                sites_only,
                only_site,
            })
            .await
        }
        Cmd::Probe {
            url,
            tls_profile,
            browser,
            cdp,
        } => {
            if browser.is_some() && cdp.is_some() {
                return Err(apperr::usage(
                    "--browser와 --cdp는 동시에 못 쓴다",
                    "하나만 지정",
                ));
            }
            let core = runner(&deps)?;
            let (result, _) = if cdp.is_some() {
                return Err(apperr::usage(
                    "CDP attach는 대시보드의 Accounts 페어링을 사용한다",
                    "hycli open",
                ));
            } else if browser.is_some() {
                core.probe_browser_stealth(&deps.site, &url).await?
            } else {
                core.probe(&deps.site, &url, &tls_profile).await?
            };
            emit.data(&result);
            let defense = result
                .get("defense")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("none");
            if defense != "none" {
                return Err(blocked_err(defense, ""));
            }
            Ok(())
        }
        Cmd::Kb { cmd } => kb_cmd(deps, cmd, emit).await,
        Cmd::Spec { cmd } => spec_cmd(deps, cmd, emit).await,
        Cmd::Auth { cmd } => auth_cmd(cmd, emit),
        Cmd::Capture {
            from,
            site,
            base,
            out,
            limit,
        } => capture_run(deps, from, site, base, out, limit, emit).await,
        Cmd::Try { site, op, args } => try_run(deps, site, op, args, emit).await,
        Cmd::Health { site } => health_run(deps, site, emit).await,
        Cmd::Site(rest) => site_run(deps, rest, emit).await,
    }
}

async fn kb_cmd(deps: Deps, cmd: KbCmd, emit: &Emitter) -> Result<(), AppError> {
    let kb = open_kb(&deps)?;
    use hycli::kb::AttemptInput;
    match cmd {
        KbCmd::Init => {
            emit.data(
                &serde_json::json!({"path": hycli::platform::resolve_kb(deps.kb.as_deref()),
                "schema_version": kb.schema_version(), "status": "ready"}),
            );
            Ok(())
        }
        KbCmd::Next => match kb.next(&deps.site)? {
            Some(g) => {
                emit.data(&serde_json::json!({"action": "resolve-gap", "gap": g,
                    "hint": "hycli kb gap-set <id> done"}));
                Ok(())
            }
            None => {
                emit.data(&serde_json::json!({"action": "none", "detail": "열린 갭 없음 — 커버리지 충족"}));
                Ok(())
            }
        },
        KbCmd::Gaps { state } => {
            emit.data(&kb.gaps(&deps.site, &state)?);
            Ok(())
        }
        KbCmd::GapSet { id, state } => {
            kb.set_gap_state(id, &state)?;
            emit.data(&serde_json::json!({"id": id, "state": state}));
            Ok(())
        }
        KbCmd::NodeLs { kind, limit } => {
            emit.data(&kb.query_nodes(&deps.site, &kind, limit)?);
            Ok(())
        }
        KbCmd::NodeGet { kind, key } => match kb.node(&deps.site, &kind, &key)? {
            Some(n) => {
                emit.data(&n);
                Ok(())
            }
            None => Err(apperr::spec(
                format!("노드 없음: {kind} {key}"),
                "kb node-ls로 확인",
            )),
        },
        KbCmd::Graph { node_id, depth } => {
            let (nodes, edges) = kb.subgraph(node_id, depth)?;
            emit.data(&serde_json::json!({"root": node_id, "depth": depth, "nodes": nodes, "edges": edges}));
            Ok(())
        }
        KbCmd::Record {
            action,
            verdict,
            stage,
            target_key,
            status,
            defense,
        } => {
            let id = kb.record_attempt(&AttemptInput {
                site: deps.site.clone(),
                stage,
                action,
                verdict,
                target_kind: String::new(),
                target_key,
                status,
                defense,
                ..Default::default()
            })?;
            emit.data(&serde_json::json!({"id": id}));
            Ok(())
        }
        KbCmd::FindingAdd {
            statement,
            confidence,
            evidence,
        } => {
            let evs: Vec<i64> = evidence
                .split(',')
                .filter_map(|p| p.trim().parse().ok())
                .collect();
            let id = kb.add_finding(&deps.site, &statement, confidence, &evs)?;
            emit.data(&serde_json::json!({"id": id, "statement": statement, "evidence": evs}));
            Ok(())
        }
        KbCmd::FindingSearch { q } => {
            emit.data(&kb.search_findings(&deps.site, &q, 50)?);
            Ok(())
        }
        KbCmd::Stats => {
            emit.data(&kb.stats(&deps.site)?);
            Ok(())
        }
        KbCmd::Fsck => {
            let issues = kb.fsck(&deps.site)?;
            let status = if issues.is_empty() {
                "clean".to_string()
            } else {
                format!("{} issues", issues.len())
            };
            emit.data(&serde_json::json!({"status": status, "issues": issues}));
            Ok(())
        }
    }
}

async fn spec_cmd(deps: Deps, cmd: SpecCmd, emit: &Emitter) -> Result<(), AppError> {
    match cmd {
        SpecCmd::Validate { file } => {
            let sp = load_file_spec(&file)?;
            hycli::policy::validate_spec(&sp)?;
            let issues = sp.validate();
            if !issues.is_empty() {
                emit.data(&serde_json::json!({"status": format!("{} issues", issues.len()), "issues": issues}));
                return Err(apperr::spec("스펙 검증 실패", "issues 항목 수정"));
            }
            emit.data(&serde_json::json!({"status": "ok", "site": sp.site.name, "ops": sp.operations.len()}));
            Ok(())
        }
        SpecCmd::Install { file } => {
            let sp = load_file_spec(&file)?;
            hycli::policy::validate_spec(&sp)?;
            let issues = sp.validate();
            if !issues.is_empty() {
                emit.data(&serde_json::json!({"issues": issues}));
                return Err(apperr::spec("검증 실패 — 설치 중단", "issues 수정"));
            }
            let dst = hycli::platform::specs_dir().join(format!("{}.yaml", sp.site.name));
            runner(&deps)?.install(&sp)?;
            emit.data(&serde_json::json!({"installed": dst, "site": sp.site.name, "ops": sp.operations.len()}));
            Ok(())
        }
        SpecCmd::Ls => {
            let rows: Vec<serde_json::Value> = hycli_sites()
                .iter()
                .map(|(n, sp)| {
                    serde_json::json!({
                        "site": n, "title": sp.site.title, "ops": sp.operations.len()
                    })
                })
                .collect();
            emit.data(&rows);
            Ok(())
        }
        SpecCmd::Rm { site } => {
            if !hycli::util::valid_id(&site) {
                return Err(apperr::usage("Invalid website ID", "hycli spec ls"));
            }
            hycli::runtime::Runtime::open(
                &hycli::platform::data_dir(),
                Some(hycli::platform::specs_dir()),
                None,
                false,
                false,
                true,
            )?
            .remove_site(&site)?;
            emit.data(&serde_json::json!({"removed": site}));
            Ok(())
        }
    }
}

fn auth_cmd(cmd: AuthCmd, emit: &Emitter) -> Result<(), AppError> {
    let mut st = hycli::store::Store::open(&hycli::platform::store_path())?;
    match cmd {
        AuthCmd::Set { r#ref, value } => {
            let (site, key) = r#ref
                .split_once('/')
                .ok_or_else(|| apperr::usage("형식은 <site>/<key>", "예: httpbin/token"))?;
            let v = match value {
                Some(v) => v,
                None => {
                    use std::io::Read;
                    let mut s = String::new();
                    let _ = std::io::stdin().read_to_string(&mut s);
                    s.trim_end().to_string()
                }
            };
            if v.is_empty() {
                return Err(apperr::usage("빈 값", "stdin 또는 --value"));
            }
            st.set(site, key, &v)?;
            emit.data(&serde_json::json!({"stored": r#ref}));
            Ok(())
        }
        AuthCmd::Ls => {
            emit.data(&st.keys());
            Ok(())
        }
        AuthCmd::Rm { r#ref } => {
            let (site, key) = r#ref
                .split_once('/')
                .ok_or_else(|| apperr::usage("형식은 <site>/<key>", "예: httpbin/token"))?;
            if site == "accounts" || site == "providers" {
                let core = hycli::runtime::Runtime::open(
                    &hycli::platform::data_dir(),
                    Some(hycli::platform::specs_dir()),
                    None,
                    false,
                    false,
                    true,
                )?;
                if site == "accounts" {
                    core.remove_account(key)?;
                } else {
                    core.remove_provider(key)?;
                }
            } else {
                st.delete(site, key)?;
            }
            emit.data(&serde_json::json!({"removed": r#ref}));
            Ok(())
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn capture_run(
    deps: Deps,
    from: String,
    site: String,
    base: String,
    out: Option<String>,
    limit: usize,
    emit: &Emitter,
) -> Result<(), AppError> {
    let kb = open_kb(&deps)?;
    let raw = std::fs::read(&from)
        .map_err(|e| apperr::spec(format!("HAR 읽기 실패 ({e})"), "경로 확인"))?;
    let f = hycli::har::parse(&raw)?;
    let clusters = hycli::capture::from_har(&f);
    if clusters.is_empty() {
        return Err(apperr::spec(
            "클러스터 0개 — XHR/JSON 응답이 없다",
            "브라우저 캡처 범위 확인",
        ));
    }
    let draft = hycli::capture::draft_spec(&site, &site, &base, &clusters, limit)
        .map_err(|e| apperr::spec(format!("{e}"), "draft.yaml을 수동 편집"))?;
    let out_path = out.unwrap_or_else(|| format!("{site}-draft.yaml"));
    let yb =
        serde_yaml::to_string(&draft).map_err(|e| apperr::runtime(e, "yaml 직렬화 실패", ""))?;
    std::fs::write(&out_path, yb).map_err(|e| apperr::runtime(e, "초안 쓰기 실패", ""))?;

    let (aid, sha) = kb.register_artifact(&site, "har", &from)?;
    let mut recorded = 0;
    for cl in &clusters {
        let ep_key = format!("{} {}", cl.method, cl.template);
        let ep_id = kb.ensure_node(
            &site,
            "endpoint",
            &ep_key,
            &serde_json::json!({"count": cl.count, "json": cl.is_json, "last": cl.last}),
        )?;
        let verdict = if cl.last >= 400 { "fail" } else { "pass" };
        kb.record_attempt(&hycli::kb::AttemptInput {
            site: site.clone(),
            stage: "recon".into(),
            action: "capture".into(),
            verdict: verdict.into(),
            target_kind: "endpoint".into(),
            target_key: ep_key.clone(),
            status: cl.last as i64,
            ..Default::default()
        })?;
        kb.add_gap(
            &site,
            "replay-unverified",
            &ep_key,
            "관찰됨, 리플레이 미검증",
            0.8,
        )?;
        kb.link(aid, ep_id, "discovered_via")?;
        recorded += 1;
    }
    let short: String = sha.chars().take(8).collect();
    emit.data(
        &serde_json::json!({"clusters": clusters.len(), "recorded": recorded, "artifact_sha": short,
        "draft": out_path, "next": "hycli try로 리플레이 검증 후 갭 done"}),
    );
    Ok(())
}

async fn try_run(
    deps: Deps,
    site: String,
    op: String,
    arg_list: Vec<String>,
    emit: &Emitter,
) -> Result<(), AppError> {
    let sp = find_spec(&site)?;
    sp.op(&op)?;
    let args = hycli::cli_actions::pairs(&arg_list)?;
    let runner = runner(&deps)?;
    let (res, _) = runner.run_op(&sp, &op, &args).await?;
    emit.data(&res);
    if res.defense != "none" && !res.defense.is_empty() {
        return Err(blocked_err(&res.defense, &res.signal));
    }
    Ok(())
}

async fn health_run(deps: Deps, site: String, emit: &Emitter) -> Result<(), AppError> {
    let sp = find_spec(&site)?;
    let usable = |op: &&hycli::spec::Operation| {
        hycli::policy::operation_effect(op, &Default::default()) == hycli::policy::Effect::Read
            && hycli::runtime::params(op).iter().all(|(_, p)| !p.required)
    };
    let op = if let Some(health) = &sp.health {
        Some(sp.op(&health.op)?)
    } else {
        sp.operations.iter().find(usable)
    }
    .filter(usable)
    .ok_or_else(|| {
        apperr::usage(
            "No read-only health action is available without inputs",
            format!("Use `hycli {site} --help` and run a read with its required inputs."),
        )
    })?
    .name
    .clone();
    let runner = runner(&deps)?;
    let (res, _) = runner.run_op(&sp, &op, &Default::default()).await?;
    emit.data(
        &serde_json::json!({"site": sp.site.name, "op": op, "status": res.status,
        "defense": res.defense, "latency_ms": res.latency_ms}),
    );
    if res.defense != "none" && !res.defense.is_empty() {
        return Err(blocked_err(&res.defense, &res.signal));
    }
    Ok(())
}

/// Installed website commands share parsing and execution with explicit `run`.
async fn site_run(mut deps: Deps, rest: Vec<String>, emit: &Emitter) -> Result<(), AppError> {
    let site = rest
        .first()
        .ok_or_else(|| apperr::usage("Choose a website", "hycli spec ls"))?;
    let sp = find_spec(site)?;
    let help = rest.len() == 1 || rest.iter().skip(1).any(|v| v == "--help" || v == "-h");
    if help {
        let core = runner(&deps)?;
        let view = core
            .sites(&core.state.read()?.settings.locale)?
            .into_iter()
            .find(|s| s.id == *site)
            .ok_or_else(|| AppError::api("not_found", 404))?;
        let action = rest
            .get(1)
            .filter(|s| !s.starts_with('-'))
            .map(String::as_str);
        print!("{}", hycli::cli_actions::help(&view, action)?);
        return Ok(());
    }
    let op = &rest[1];
    let args = hycli::cli_actions::parse(sp.op(op)?, &rest[2..])?;
    deps.allow_local |= args.allow_local;
    let (result, _) = runner(&deps)?
        .run_op_for_account(&sp, op, &args.inputs, args.account.as_deref())
        .await?;
    emit.data(&result.mapped.or(result.body));
    Ok(())
}

async fn wait_preparation(
    core: &std::sync::Arc<hycli::runtime::Runtime>,
    id: &str,
    emit: &Emitter,
) -> Result<(), AppError> {
    let wait = async {
        let mut latest = String::new();
        loop {
            let mut job = core.state.job(id)?;
            if job.updated_at != latest {
                if let Some(note) = job.notes.last() {
                    eprintln!("hycli: {}", note.message);
                }
                latest = job.updated_at.clone();
            }
            if !["running", "queued"].contains(&job.status.as_str()) {
                job.url.clear();
                emit.data(&job);
                return job_outcome(&job);
            }
            tokio::time::sleep(std::time::Duration::from_millis(750)).await;
        }
    };
    tokio::select! {
        result = wait => result,
        _ = tokio::signal::ctrl_c() => { core.cancel_job(id)?; Err(AppError::api("cancelled", 409)) }
    }
}

fn job_outcome(job: &hycli::state::Job) -> Result<(), AppError> {
    match job.status.as_str() {
        "completed" | "running" | "queued" => Ok(()),
        _ => {
            let code = if job.error_code.is_empty() {
                job.status.as_str()
            } else {
                &job.error_code
            };
            let status = match code {
                "auth_required" | "identity_unavailable" => 401,
                "blocked_by_site" | "site_rate_limited" => 429,
                _ => 500,
            };
            Err(AppError::api(code, status))
        }
    }
}

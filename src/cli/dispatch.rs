use super::{
    actions,
    args::{Cmd, Deps, JobsCmd, KbCmd},
    site_cmd::{
        auth_cmd, capture_run, check_run, find_spec, hycli_sites, job_outcome, site_run, spec_cmd,
        try_run, wait_preparation,
    },
};
use crate::{
    apperr::{self, AppError},
    output::Emitter,
};

pub async fn run(deps: Deps, cmd: Cmd, emit: &Emitter) -> i32 {
    match dispatch(deps, cmd, emit).await {
        Ok(()) => 0,
        Err(e) => emit.fail(&e),
    }
}

pub(super) fn runner(deps: &Deps) -> Result<std::sync::Arc<crate::runtime::Runtime>, AppError> {
    runtime(deps, false, deps.allow_local)
}

pub(super) fn runtime(
    deps: &Deps,
    recover: bool,
    allow_local: bool,
) -> Result<std::sync::Arc<crate::runtime::Runtime>, AppError> {
    crate::runtime::Runtime::open(
        &crate::platform::data_dir(),
        Some(crate::platform::specs_dir()),
        Some(crate::platform::resolve_kb(deps.kb.as_deref())),
        recover,
        allow_local,
        true,
    )
}

pub(super) fn open_kb(deps: &Deps) -> Result<std::sync::Arc<crate::kb::Kb>, AppError> {
    Ok(std::sync::Arc::new(crate::kb::Kb::open(
        &crate::platform::resolve_kb(deps.kb.as_deref()),
    )?))
}

pub(super) fn blocked_err(defense: &str, signal: &str) -> AppError {
    let cls = match defense {
        "cloudflare" => crate::defend::Class::Cloudflare,
        "bot-manager" => crate::defend::Class::BotManager,
        "rate-limit" => crate::defend::Class::RateLimit,
        "captcha" => crate::defend::Class::Captcha,
        _ => crate::defend::Class::Unknown,
    };
    let sig = if signal.is_empty() {
        String::new()
    } else {
        format!(" ({signal})")
    };
    apperr::blocked(format!("Access blocked: {defense}{sig}"), cls.remedy())
}

async fn dispatch(deps: Deps, cmd: Cmd, emit: &Emitter) -> Result<(), AppError> {
    match cmd {
        Cmd::Browsers { url } => {
            if let Some(url) = url {
                emit.data(&runner(&deps)?.find_browser_accounts("", &url, "").await?);
            } else {
                emit.data(&crate::browser_discovery::discover());
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
        Cmd::Feedback => {
            let url = crate::feedback::issue_url();
            // The platform opener is asynchronous: spawning it does not prove a page opened.
            let launch_requested = crate::dashboard::open_url(&url).is_ok();
            emit.data(&serde_json::json!({"url": url, "launch_requested": launch_requested}));
            if launch_requested {
                eprintln!(
                    "hycli: browser launch requested; if no page opens, open the returned URL"
                );
            } else {
                eprintln!(
                    "hycli: browser launcher unavailable; open the returned URL to send feedback"
                );
            }
            Ok(())
        }
        Cmd::Describe { site, action } => {
            // Diagnose damaged definitions instead of silently losing them from discovery.
            let _ = hycli_sites();
            if let Some(id) = &site {
                find_spec(id)?;
            }
            let core = runner(&deps)?;
            let sites = core.sites(&core.state.settings()?.locale)?;
            if let Some(id) = site {
                let site = sites
                    .into_iter()
                    .find(|s| s.id == id)
                    .ok_or_else(|| AppError::api("not_found", 404))?;
                if let Some(id) = action {
                    let names: Vec<String> = site.actions.iter().map(|a| a.id.clone()).collect();
                    let action =
                        site.actions
                            .into_iter()
                            .find(|a| a.id == id)
                            .ok_or_else(|| {
                                let refs: Vec<&str> = names.iter().map(String::as_str).collect();
                                let suggestion = actions::closest_match(&id, refs)
                                    .map(|best| format!("Did you mean '{best}'? "))
                                    .unwrap_or_default();
                                apperr::usage(
                                    format!("Unknown action: {id}"),
                                    format!("{suggestion}Available actions: {}.", names.join(", ")),
                                )
                            })?;
                    emit.data(&action);
                } else {
                    let latest_understanding =
                        core.state.latest_understanding(&site.id).ok().flatten();
                    let guide = actions::build_user_guide(&site, latest_understanding.as_ref());
                    let mut val = serde_json::to_value(&site).unwrap_or(serde_json::Value::Null);
                    if let Some(obj) = val.as_object_mut() {
                        obj.insert(
                            "user_guide".into(),
                            serde_json::to_value(&guide).unwrap_or(serde_json::Value::Null),
                        );
                    }
                    emit.data(&val);
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
                .accounts()?
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
            let locale = language.unwrap_or(core.state.settings()?.locale);
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
            let locale = language.unwrap_or(core.state.settings()?.locale);
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
            if sp.op(&action).is_err() {
                let names: Vec<&str> = sp.operations.iter().map(|o| o.name.as_str()).collect();
                let suggestion = actions::closest_match(&action, names.iter().copied())
                    .map(|best| format!("Did you mean '{best}'? "))
                    .unwrap_or_default();
                return Err(apperr::usage(
                    format!("Unknown action: {action}"),
                    format!(
                        "{suggestion}Available actions: {}. Run `hycli {site} --help`.",
                        names.join(", ")
                    ),
                ));
            }
            let args = actions::pairs(&args)?;
            let (result, _) = runner(&deps)?
                .run_op_for_account(&sp, &action, &args, account.as_deref())
                .await?;
            emit.data(&result.mapped.or(result.body));
            Ok(())
        }
        Cmd::Writes { site, mode } => {
            find_spec(&site)?;
            let core = runner(&deps)?;
            let writes = match mode.as_deref().map(str::to_ascii_lowercase).as_deref() {
                None | Some("status") => core
                    .state
                    .read()?
                    .sites
                    .get(&site)
                    .is_some_and(|m| m.writes),
                Some("allow" | "on" | "true" | "enable" | "yes" | "1") => {
                    core.set_writes(&site, true)?;
                    true
                }
                Some("deny" | "off" | "false" | "disable" | "no" | "0") => {
                    core.set_writes(&site, false)?;
                    false
                }
                Some(other) => {
                    return Err(apperr::usage(
                        format!("Unknown writes mode: {other}"),
                        format!("Use `hycli writes {site} allow` or `hycli writes {site} deny`."),
                    ));
                }
            };
            emit.data(&serde_json::json!({
                "site": site,
                "writes": writes,
            }));
            Ok(())
        }
        Cmd::Open { port, no_open } => {
            let instance =
                crate::desktop::open(port, !no_open, deps.allow_local, deps.kb.as_deref()).await?;
            emit.data(&serde_json::json!({"running": true, "url": instance.url()}));
            Ok(())
        }
        Cmd::Status => {
            let instance = crate::desktop::running(&crate::platform::data_dir()).await;
            emit.data(&serde_json::json!({"running": instance.is_some(), "url": instance.map(|i| i.url())}));
            Ok(())
        }
        Cmd::Stop => {
            crate::desktop::stop(&crate::platform::data_dir()).await?;
            emit.data(&serde_json::json!({"running": false}));
            Ok(())
        }
        Cmd::Dashboard {
            port,
            no_open,
            auto_port,
        } => {
            crate::dashboard::serve_with_options(
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
            crate::guarded_mcp::serve(crate::guarded_mcp::Server {
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
                    "--browser and --cdp cannot be used together",
                    "Specify only one.",
                ));
            }
            let core = runner(&deps)?;
            let (result, _) = if cdp.is_some() {
                return Err(apperr::usage(
                    "Use Accounts pairing in the dashboard for browser sessions",
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
        Cmd::Recon {
            url,
            draft,
            max_bundles,
        } => {
            let core = runner(&deps)?;
            let (mut report, spec) = core.recon(&deps.site, &url, max_bundles.min(24)).await?;
            if let Some(obj) = report.as_object_mut() {
                let actions: Vec<serde_json::Value> = spec
                    .operations
                    .iter()
                    .map(|op| {
                        serde_json::json!({
                            "id": op.name,
                            "method": op.method,
                            "path": op.path,
                            "effect": op.effect,
                            "example": format!("hycli {} {}", spec.site.name, op.name),
                        })
                    })
                    .collect();
                obj.insert("discovered_actions".into(), serde_json::json!(actions));
                let next = if let Some(ref out_path) = draft {
                    vec![
                        format!("hycli spec validate {out_path}"),
                        format!("hycli spec install {out_path}"),
                        format!("hycli {} --help", spec.site.name),
                    ]
                } else {
                    vec![
                        format!("hycli recon {url} --draft {}-draft.yaml", spec.site.name),
                        format!("hycli prepare {url}"),
                    ]
                };
                obj.insert("next_steps".into(), serde_json::json!(next));
            }
            if let Some(out_path) = draft {
                let yaml = serde_yaml::to_string(&spec)
                    .map_err(|e| apperr::spec(format!("Failed to serialize YAML ({e})"), ""))?;
                std::fs::write(&out_path, yaml)
                    .map_err(|e| apperr::runtime(e, "Failed to write file", &out_path))?;
                if let Some(obj) = report.as_object_mut() {
                    obj.insert("draft_file".into(), serde_json::Value::String(out_path));
                }
            }
            emit.data(&report);
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
        Cmd::Check { site } => check_run(deps, site, emit).await,
        Cmd::Site(rest) => site_run(deps, rest, emit).await,
    }
}

async fn kb_cmd(deps: Deps, cmd: KbCmd, emit: &Emitter) -> Result<(), AppError> {
    let kb = open_kb(&deps)?;
    use crate::kb::AttemptInput;
    match cmd {
        KbCmd::Init => {
            emit.data(
                &serde_json::json!({"path": crate::platform::resolve_kb(deps.kb.as_deref()),
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
                emit.data(&serde_json::json!({"action": "none", "detail": "No open gaps; coverage satisfied"}));
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
                format!("Node not found: {kind} {key}"),
                "Check with hycli kb node-ls.",
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

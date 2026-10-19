use super::{
    actions,
    args::{AuthCmd, Deps, SpecCmd},
    dispatch::{blocked_err, open_kb, runner},
};
use crate::{
    apperr::{self, AppError},
    output::Emitter,
};

pub(super) fn find_spec(name: &str) -> Result<crate::spec::Spec, AppError> {
    if !crate::util::valid_id(name) {
        return Err(apperr::usage(
            "Invalid website ID",
            "Use `hycli spec ls` to list installed websites.",
        ));
    }
    let path = crate::platform::specs_dir().join(format!("{name}.yaml"));
    if !path.exists() {
        return Err(apperr::spec(
            format!("Website is not installed: {name}"),
            "Use `hycli prepare URL`, open `hycli open`, or install a definition with `hycli spec install FILE`.",
        ));
    }
    let spec = crate::spec::parse(&crate::util::read_bounded(&path, 2 * 1024 * 1024)?)?;
    crate::policy::validate_spec(&spec).map_err(|_| {
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

pub(super) fn hycli_sites() -> Vec<(String, crate::spec::Spec)> {
    let dir = crate::platform::specs_dir();
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

pub(super) fn load_file_spec(file: &str) -> Result<crate::spec::Spec, AppError> {
    let data = std::fs::read(file)
        .map_err(|e| apperr::spec(format!("Failed to read file ({e})"), "Check the file path."))?;
    crate::spec::parse(&data)
}

pub(super) async fn spec_cmd(deps: Deps, cmd: SpecCmd, emit: &Emitter) -> Result<(), AppError> {
    match cmd {
        SpecCmd::Validate { file } => {
            let sp = load_file_spec(&file)?;
            let issues = sp.validate();
            if !issues.is_empty() {
                emit.data(&serde_json::json!({"status": format!("{} issues", issues.len()), "issues": issues}));
                return Err(apperr::spec(
                    "Spec validation failed",
                    "Fix the reported issues.",
                ));
            }
            crate::policy::validate_spec(&sp)?;
            emit.data(&serde_json::json!({"status": "ok", "site": sp.site.name, "ops": sp.operations.len()}));
            Ok(())
        }
        SpecCmd::Install { file } => {
            let sp = load_file_spec(&file)?;
            let issues = sp.validate();
            if !issues.is_empty() {
                emit.data(&serde_json::json!({"issues": issues}));
                return Err(apperr::spec(
                    "Validation failed; installation aborted",
                    "Fix the reported issues.",
                ));
            }
            crate::policy::validate_spec(&sp)?;
            let dst = crate::platform::specs_dir().join(format!("{}.yaml", sp.site.name));
            let core = runner(&deps)?;
            core.install(&sp)?;
            let locale = core.state.settings()?.locale;
            let user_guide = core
                .sites(&locale)
                .ok()
                .and_then(|sites| sites.into_iter().find(|s| s.id == sp.site.name))
                .map(|site| actions::build_user_guide(&site, None));
            if let Some(guide) = &user_guide {
                eprint!("{}", actions::format_user_briefing(guide));
            }
            emit.data(&serde_json::json!({
                "installed": dst,
                "site": sp.site.name,
                "ops": sp.operations.len(),
                "user_guide": user_guide,
            }));
            Ok(())
        }
        SpecCmd::Ls => {
            let rows: Vec<serde_json::Value> = hycli_sites()
                .iter()
                .map(|(n, sp)| {
                    let read_ops = sp
                        .operations
                        .iter()
                        .filter(|o| {
                            crate::policy::operation_effect(o, &std::collections::BTreeMap::new())
                                == crate::policy::Effect::Read
                        })
                        .count();
                    let write_ops = sp.operations.len().saturating_sub(read_ops);
                    let sample_actions: Vec<&str> = sp
                        .operations
                        .iter()
                        .take(5)
                        .map(|o| o.name.as_str())
                        .collect();
                    let example_command = sp
                        .operations
                        .first()
                        .map(|o| format!("hycli {n} {}", o.name))
                        .unwrap_or_else(|| format!("hycli {n} --help"));
                    serde_json::json!({
                        "site": n,
                        "title": sp.site.title,
                        "ops": sp.operations.len(),
                        "read_ops": read_ops,
                        "write_ops": write_ops,
                        "sample_actions": sample_actions,
                        "example_command": example_command,
                    })
                })
                .collect();
            emit.data(&rows);
            Ok(())
        }
        SpecCmd::Rm { site } => {
            if !crate::util::valid_id(&site) {
                return Err(apperr::usage("Invalid website ID", "hycli spec ls"));
            }
            crate::runtime::Runtime::open(
                &crate::platform::data_dir(),
                Some(crate::platform::specs_dir()),
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

pub(super) fn auth_cmd(cmd: AuthCmd, emit: &Emitter) -> Result<(), AppError> {
    let mut st = crate::store::Store::open(&crate::platform::store_path())?;
    match cmd {
        AuthCmd::Set { r#ref, value } => {
            let (site, key) = r#ref.split_once('/').ok_or_else(|| {
                apperr::usage("Format must be <site>/<key>", "Example: httpbin/token")
            })?;
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
                return Err(apperr::usage(
                    "Empty credential value",
                    "Provide via stdin or --value.",
                ));
            }
            if let Ok(core) = crate::runtime::Runtime::open(
                &crate::platform::data_dir(),
                Some(crate::platform::specs_dir()),
                None,
                false,
                false,
                true,
            ) {
                if core.set_website_credential(site, key, Some(&v)).is_err() {
                    st.set(site, key, &v)?;
                    let _ = core.policy.invalidate(Some(site), None);
                }
            } else {
                st.set(site, key, &v)?;
            }
            emit.data(&serde_json::json!({"stored": r#ref}));
            Ok(())
        }
        AuthCmd::Ls => {
            emit.data(&st.keys());
            Ok(())
        }
        AuthCmd::Rm { r#ref } => {
            let (site, key) = r#ref.split_once('/').ok_or_else(|| {
                apperr::usage("Format must be <site>/<key>", "Example: httpbin/token")
            })?;
            if site == "accounts" || site == "providers" {
                let core = crate::runtime::Runtime::open(
                    &crate::platform::data_dir(),
                    Some(crate::platform::specs_dir()),
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
            } else if let Ok(core) = crate::runtime::Runtime::open(
                &crate::platform::data_dir(),
                Some(crate::platform::specs_dir()),
                None,
                false,
                false,
                true,
            ) {
                if core.set_website_credential(site, key, None).is_err() {
                    st.delete(site, key)?;
                    let _ = core.policy.invalidate(Some(site), None);
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
pub(super) async fn capture_run(
    deps: Deps,
    from: String,
    site: String,
    base: String,
    out: Option<String>,
    limit: usize,
    emit: &Emitter,
) -> Result<(), AppError> {
    let kb = open_kb(&deps)?;
    let raw = std::fs::read(&from).map_err(|e| {
        apperr::spec(
            format!("Failed to read HAR file ({e})"),
            "Check the file path.",
        )
    })?;
    let f = crate::har::parse(&raw)?;
    let clusters = crate::capture::from_har(&f);
    if clusters.is_empty() {
        return Err(apperr::spec(
            "Zero endpoint clusters found; no XHR/JSON responses in HAR",
            "Check the browser capture scope.",
        ));
    }
    let draft = crate::capture::draft_spec(&site, &site, &base, &clusters, limit)
        .map_err(|e| apperr::spec(format!("{e}"), "Edit draft.yaml manually."))?;
    let out_path = out.unwrap_or_else(|| format!("{site}-draft.yaml"));
    let yb = serde_yaml::to_string(&draft)
        .map_err(|e| apperr::runtime(e, "Failed to serialize YAML", ""))?;
    std::fs::write(&out_path, yb).map_err(|e| apperr::runtime(e, "Failed to write draft", ""))?;

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
        kb.record_attempt(&crate::kb::AttemptInput {
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
            "Observed in HAR; replay unverified",
            0.8,
        )?;
        kb.link(aid, ep_id, "discovered_via")?;
        recorded += 1;
    }
    let short: String = sha.chars().take(8).collect();
    emit.data(
        &serde_json::json!({"clusters": clusters.len(), "recorded": recorded, "artifact_sha": short,
        "draft": out_path, "next": "Verify replay with hycli try, then mark gap done"}),
    );
    Ok(())
}

pub(super) async fn try_run(
    deps: Deps,
    site: String,
    op: String,
    arg_list: Vec<String>,
    emit: &Emitter,
) -> Result<(), AppError> {
    let args = actions::pairs(&arg_list)?;
    let runner = runner(&deps)?;
    let (res, _) = if (site.ends_with(".yaml")
        || site.ends_with(".yml")
        || std::path::Path::new(&site).exists())
        && !crate::util::valid_id(&site)
    {
        let sp = load_file_spec(&site)?;
        sp.op(&op)?;
        runner
            .verify_candidate_with_inputs(&sp, &op, "", args)
            .await?
    } else {
        let sp = find_spec(&site)?;
        sp.op(&op)?;
        runner.run_op(&sp, &op, &args).await?
    };
    emit.data(&res);
    if res.defense != "none" && !res.defense.is_empty() {
        return Err(blocked_err(&res.defense, &res.signal));
    }
    Ok(())
}

pub(super) async fn check_run(
    deps: Deps,
    site: Option<String>,
    emit: &Emitter,
) -> Result<(), AppError> {
    let core = runner(&deps)?;
    let sites = match site {
        Some(site) => vec![find_spec(&site)?.site.name],
        None => core.specs()?.into_iter().map(|sp| sp.site.name).collect(),
    };
    let mut reports = vec![];
    for site in sites {
        reports.push(core.check_site(&site).await?);
    }
    emit.data(&reports);
    // Exit status follows the most actionable verdict, so scripts can branch without parsing.
    let worst = [
        "signed_out",
        "unknown",
        "site_changed",
        "blocked",
        "unreachable",
        "no_checks",
    ]
    .into_iter()
    .find(|verdict| reports.iter().any(|report| report.verdict == *verdict));
    match worst {
        None => Ok(()),
        Some("signed_out" | "unknown") => Err(apperr::auth_err(
            "A website session has expired or could not be confirmed",
            "Reconnect the account in `hycli dashboard`, then run `hycli check` again.",
        )),
        Some("site_changed") => Err(apperr::spec(
            "A website no longer returns what its actions expect",
            "Run `hycli request SITE \"Repair the failing reads\"` or prepare the website again.",
        )),
        Some("blocked") => Err(apperr::blocked(
            "A website is limiting or challenging requests",
            "Wait, or resolve the challenge in your browser.",
        )),
        Some("no_checks") => Err(apperr::usage(
            "No recorded read is available to check",
            "Prepare the website again to record its checks.",
        )),
        Some(_) => Err(apperr::runtime_msg(
            "A website could not be reached",
            "Check the connection and retry later.",
        )),
    }
}

/// Installed website commands share parsing and execution with explicit `run`.
pub(super) async fn site_run(
    mut deps: Deps,
    rest: Vec<String>,
    emit: &Emitter,
) -> Result<(), AppError> {
    let site = rest
        .first()
        .ok_or_else(|| apperr::usage("Choose a website", "hycli spec ls"))?;
    let sp = find_spec(site)?;
    let help = rest.len() == 1 || rest.iter().skip(1).any(|v| v == "--help" || v == "-h");
    if help {
        let core = runner(&deps)?;
        let view = core
            .sites(&core.state.settings()?.locale)?
            .into_iter()
            .find(|s| s.id == *site)
            .ok_or_else(|| AppError::api("not_found", 404))?;
        let action = rest
            .get(1)
            .filter(|s| !s.starts_with('-'))
            .map(String::as_str);
        print!("{}", actions::help(&view, action)?);
        return Ok(());
    }
    let op = &rest[1];
    let operation = sp.op(op).map_err(|_| {
        let names: Vec<&str> = sp.operations.iter().map(|o| o.name.as_str()).collect();
        let suggestion = actions::closest_match(op, names.iter().copied())
            .map(|best| format!("Did you mean '{best}'? "))
            .unwrap_or_default();
        apperr::usage(
            format!("Unknown action: {op}"),
            format!(
                "{suggestion}Available actions: {}. Run `hycli {site} --help`.",
                names.join(", ")
            ),
        )
    })?;
    let args = actions::parse(operation, &rest[2..])?;
    deps.allow_local |= args.allow_local;
    let (result, _) = runner(&deps)?
        .run_op_for_account(&sp, op, &args.inputs, args.account.as_deref())
        .await?;
    emit.data(&result.mapped.or(result.body));
    Ok(())
}

pub(super) async fn wait_preparation(
    core: &std::sync::Arc<crate::runtime::Runtime>,
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
                if job.status == "completed" {
                    if let Some(guide) = job
                        .result
                        .as_ref()
                        .and_then(|r| r.get("user_guide"))
                        .and_then(|v| serde_json::from_value::<actions::UserGuide>(v.clone()).ok())
                    {
                        eprint!("{}", actions::format_user_briefing(&guide));
                    }
                }
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

pub(super) fn job_outcome(job: &crate::state::Job) -> Result<(), AppError> {
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

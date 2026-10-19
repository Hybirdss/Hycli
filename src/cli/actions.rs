//! Installed commands use the same typed inputs as the dashboard and MCP.
use crate::{
    apperr::{self, AppResult},
    runtime,
    spec::Operation,
};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Arguments {
    pub inputs: BTreeMap<String, String>,
    pub account: Option<String>,
    pub allow_local: bool,
}

pub fn pairs(values: &[String]) -> AppResult<BTreeMap<String, String>> {
    let mut inputs = BTreeMap::new();
    for value in values {
        let (name, value) = value
            .split_once('=')
            .ok_or_else(|| apperr::usage("Expected --arg name=value", "Example: --arg q=design"))?;
        insert(&mut inputs, name, value.into())?;
    }
    Ok(inputs)
}

fn insert(inputs: &mut BTreeMap<String, String>, name: &str, value: String) -> AppResult<()> {
    if name.is_empty() || inputs.insert(name.into(), value).is_some() {
        return Err(apperr::usage(
            format!("Repeated or empty input: {name}"),
            "Provide each input once.",
        ));
    }
    Ok(())
}

pub fn parse(op: &Operation, flags: &[String]) -> AppResult<Arguments> {
    let known = runtime::params(op);
    let mut out = Arguments::default();
    let mut index = 0;
    while index < flags.len() {
        let flag = flags[index].strip_prefix("--").ok_or_else(|| {
            apperr::usage(
                format!("Expected a named input: {}", flags[index]),
                "Use --name value or --name=value.",
            )
        })?;
        let (name, inline) = flag
            .split_once('=')
            .map_or((flag, None), |(k, v)| (k, Some(v)));
        if name == "allow-local"
            && inline.is_none()
            && !known.iter().any(|(k, _)| k.as_str() == name)
        {
            out.allow_local = true;
            index += 1;
            continue;
        }
        // Preserve literal input names before considering the kebab-case convenience alias.
        let alias = name.replace('-', "_");
        let input = known
            .iter()
            .find(|(k, _)| k.as_str() == name)
            .or_else(|| known.iter().find(|(k, _)| k.as_str() == alias));
        let account = name == "hycli-account" || (name == "account" && input.is_none());
        if input.is_none() && !account {
            let candidates: Vec<String> = known.iter().map(|(k, _)| k.replace('_', "-")).collect();
            let refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
            let suggestion = closest_match(name, refs)
                .map(|best| format!("Did you mean --{best}? "))
                .unwrap_or_default();
            let available = if known.is_empty() {
                "This action takes no inputs.".into()
            } else {
                let list: Vec<String> = known
                    .iter()
                    .map(|(k, p)| {
                        format!(
                            "--{} <{}>{}",
                            k.replace('_', "-"),
                            p.kind,
                            if p.required { " (required)" } else { "" }
                        )
                    })
                    .collect();
                format!("Available inputs: {}.", list.join(", "))
            };
            return Err(apperr::usage(
                format!("Unknown input: --{name}"),
                format!("{suggestion}{available} Use the action's --help to list its inputs."),
            ));
        }
        let value = if let Some(value) = inline {
            value.to_string()
        } else if !account
            && input.is_some_and(|(_, p)| p.kind == "bool")
            && flags
                .get(index + 1)
                .is_none_or(|value| value.starts_with("--"))
        {
            "true".into()
        } else {
            index += 1;
            flags
                .get(index)
                .filter(|value| !value.starts_with("--"))
                .cloned()
                .ok_or_else(|| {
                    apperr::usage(
                        format!("Missing value for --{name}"),
                        "Use --name=value for a value beginning with --.",
                    )
                })?
        };
        if account {
            if out.account.replace(value).is_some() {
                return Err(apperr::usage(
                    "Account specified more than once",
                    "Use --hycli-account ID once.",
                ));
            }
        } else {
            insert(&mut out.inputs, input.unwrap().0, value)?;
        }
        index += 1;
    }
    Ok(out)
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ActionCapability {
    pub id: String,
    pub title: String,
    pub description: String,
    pub effect: String,
    pub verified: bool,
    pub required_inputs: Vec<String>,
    pub optional_inputs: Vec<String>,
    pub example_command: String,
    pub natural_prompt: String,
    pub expected_output: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct WorkflowCapability {
    pub id: String,
    pub title: String,
    pub benefit: String,
    pub steps: Vec<String>,
    pub success_criteria: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct UserGuide {
    pub site_id: String,
    pub site_title: String,
    pub summary: String,
    pub headline: String,
    pub writes_enabled: bool,
    pub enable_writes_command: String,
    pub agent_instruction: String,
    pub read_capabilities: Vec<ActionCapability>,
    pub write_capabilities: Vec<ActionCapability>,
    pub workflows: Vec<WorkflowCapability>,
    pub quick_start_commands: Vec<String>,
    pub extend_command: String,
}

pub fn sample_input_value(field: &crate::runtime::FieldView) -> String {
    if let Some(default) = &field.default {
        match default {
            serde_json::Value::String(s) if !s.is_empty() => {
                return if s.chars().any(char::is_whitespace) || s.contains('"') || s.contains('\'')
                {
                    format!("\"{}\"", s.replace('"', "\\\""))
                } else {
                    s.clone()
                };
            }
            serde_json::Value::Number(n) => return n.to_string(),
            serde_json::Value::Bool(b) => return b.to_string(),
            _ => {}
        }
    }
    match field.kind.as_str() {
        "int" => "1".into(),
        "float" => "1.0".into(),
        "bool" => "true".into(),
        "json" => "'{}'".into(),
        _ => {
            let lower = field.id.to_ascii_lowercase();
            if lower.contains("id") {
                "\"<id>\"".into()
            } else if lower == "q" || lower.contains("query") || lower.contains("search") {
                "\"search-term\"".into()
            } else if lower.contains("url") {
                "\"https://example.com\"".into()
            } else {
                format!("\"<{}>\"", field.id)
            }
        }
    }
}

pub fn sample_command(site_id: &str, op: &crate::runtime::ActionView) -> String {
    let mut cmd = format!("hycli {site_id} {}", op.id);
    for field in op.inputs.iter().filter(|f| f.required).take(4) {
        let flag = field.id.replace('_', "-");
        if field.kind == "bool" {
            cmd.push_str(&format!(" --{flag}"));
        } else {
            cmd.push_str(&format!(" --{flag} {}", sample_input_value(field)));
        }
    }
    cmd
}

pub fn build_user_guide(
    site: &crate::runtime::SiteView,
    understanding: Option<&crate::state::WebsiteUnderstanding>,
) -> UserGuide {
    let mut read_capabilities = Vec::new();
    let mut write_capabilities = Vec::new();
    let mut quick_start_commands = Vec::new();

    for op in &site.actions {
        let required_inputs: Vec<String> = op
            .inputs
            .iter()
            .filter(|f| f.required)
            .map(|f| {
                if f.label.is_empty() {
                    format!("--{} <{}>", f.id.replace('_', "-"), f.kind)
                } else {
                    format!("--{} <{}> ({})", f.id.replace('_', "-"), f.kind, f.label)
                }
            })
            .collect();
        let optional_inputs: Vec<String> = op
            .inputs
            .iter()
            .filter(|f| !f.required)
            .map(|f| {
                let def = f
                    .default
                    .as_ref()
                    .map(|v| format!(" = {v}"))
                    .unwrap_or_default();
                format!("--{} <{}>{def}", f.id.replace('_', "-"), f.kind)
            })
            .collect();
        let example_command = sample_command(&site.id, op);
        let title = if op.title.is_empty() {
            op.id.clone()
        } else {
            op.title.clone()
        };
        let description = if op.description.is_empty() {
            title.clone()
        } else {
            op.description.clone()
        };
        let natural_prompt = format!("Use {} to {} ({})", site.title, title.to_lowercase(), op.id);
        let cap = ActionCapability {
            id: op.id.clone(),
            title,
            description,
            effect: op.effect.as_str().into(),
            verified: op.verified,
            required_inputs,
            optional_inputs,
            example_command: example_command.clone(),
            natural_prompt,
            expected_output: op.output.clone(),
        };
        if op.effect == crate::policy::Effect::Read {
            if quick_start_commands.len() < 4 {
                quick_start_commands.push(example_command);
            }
            read_capabilities.push(cap);
        } else {
            write_capabilities.push(cap);
        }
    }
    if quick_start_commands.is_empty() {
        for cap in write_capabilities.iter().take(2) {
            quick_start_commands.push(cap.example_command.clone());
        }
    }
    let workflows = understanding
        .map(|u| {
            u.workflows
                .iter()
                .map(|w| WorkflowCapability {
                    id: w.id.clone(),
                    title: w.title.clone(),
                    benefit: w.benefit.clone(),
                    steps: w.steps.clone(),
                    success_criteria: w.success_criteria.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let site_title = if site.title.is_empty() {
        site.id.clone()
    } else {
        site.title.clone()
    };
    let headline = format!(
        "{} ({}) is ready: {} read action(s) and {} write action(s).",
        site_title,
        site.id,
        read_capabilities.len(),
        write_capabilities.len()
    );
    let extend_command = format!(
        "hycli request {} \"Describe additional capabilities to add\"",
        site.id
    );
    let enable_writes_command = format!("hycli writes {} allow", site.id);
    let agent_instruction = if !write_capabilities.is_empty() && !site.writes {
        format!(
            "Present these capabilities to the user in a clear, user-friendly summary: explain what they can read and what website actions/changes are available, and share 2-3 example commands. Write actions are currently off for '{}' (default for a new CLI): ask the user ONCE for this CLI whether to allow write actions (e.g., '{} CLI에서 쓰기를 허용할까요?'), and if they confirm, run `{}` (or MCP `hycli_writes`) once to enable direct write execution.",
            site.id, site_title, enable_writes_command
        )
    } else {
        "Present these capabilities to the user in a clear, user-friendly summary: explain what they can read and what website actions/changes they can trigger directly, and share 2-3 example commands or natural-language prompts they can run right now.".into()
    };
    UserGuide {
        site_id: site.id.clone(),
        site_title,
        summary: site.summary.clone(),
        headline,
        writes_enabled: site.writes,
        enable_writes_command,
        agent_instruction,
        read_capabilities,
        write_capabilities,
        workflows,
        quick_start_commands,
        extend_command,
    }
}

pub fn format_user_briefing(guide: &UserGuide) -> String {
    let mut out = String::new();
    out.push_str(&format!("hycli: {}\n", guide.headline));
    if !guide.summary.is_empty() {
        out.push_str(&format!("  {}\n", guide.summary));
    }
    if !guide.read_capabilities.is_empty() {
        out.push_str("  Read capabilities (run immediately):\n");
        for cap in guide.read_capabilities.iter().take(8) {
            let badge = if cap.verified { " [verified]" } else { "" };
            out.push_str(&format!(
                "    - {} ({}){}: {}\n      Run: {}\n",
                cap.title, cap.id, badge, cap.description, cap.example_command
            ));
        }
    }
    if !guide.write_capabilities.is_empty() {
        if guide.writes_enabled {
            out.push_str("  Write capabilities (enabled — run directly):\n");
        } else {
            out.push_str(&format!(
                "  Write capabilities (ask user once to allow, then run `{}`):\n",
                guide.enable_writes_command
            ));
        }
        for cap in guide.write_capabilities.iter().take(6) {
            out.push_str(&format!(
                "    - {} ({}): {}\n      Run: {}\n",
                cap.title, cap.id, cap.description, cap.example_command
            ));
        }
    }
    if !guide.workflows.is_empty() {
        out.push_str("  Supported end-to-end workflows:\n");
        for wf in guide.workflows.iter().take(4) {
            out.push_str(&format!(
                "    - {}: {} ({})\n",
                wf.title,
                wf.benefit,
                wf.steps.join(" -> ")
            ));
        }
    }
    out.push_str(&format!("  Need more? {}\n", guide.extend_command));
    out
}

pub fn closest_match<'a>(
    target: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    let target_lower = target.to_ascii_lowercase().replace('_', "-");
    if target_lower.is_empty() {
        return None;
    }
    let mut best: Option<(&'a str, usize)> = None;
    for candidate in candidates {
        let cand_lower = candidate.to_ascii_lowercase().replace('_', "-");
        if cand_lower == target_lower {
            return Some(candidate);
        }
        let dist = edit_distance(&target_lower, &cand_lower);
        let prefix_bonus = if cand_lower.starts_with(&target_lower)
            || target_lower.starts_with(&cand_lower)
            || (target_lower.len() >= 3 && cand_lower.contains(&target_lower))
        {
            dist.min(2)
        } else {
            dist
        };
        let threshold = (target_lower.len().max(cand_lower.len()) / 2).max(2);
        if prefix_bonus <= threshold && best.is_none_or(|(_, best_score)| prefix_bonus < best_score)
        {
            best = Some((candidate, prefix_bonus));
        }
    }
    best.map(|(c, _)| c)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b_chars.len()).collect();
    let mut curr = vec![0; b_chars.len() + 1];
    for (i, &ca) in a_chars.iter().enumerate() {
        curr[0] = i + 1;
        for (j, &cb) in b_chars.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b_chars.len()]
}

pub fn help(site: &crate::runtime::SiteView, action: Option<&str>) -> AppResult<String> {
    let mut text = format!("{}\n{}\n\n", site.title, site.summary);
    if let Some(id) = action {
        let op = site.actions.iter().find(|op| op.id == id).ok_or_else(|| {
            let names: Vec<&str> = site.actions.iter().map(|a| a.id.as_str()).collect();
            let suggestion = closest_match(id, names.iter().copied())
                .map(|best| format!("Did you mean '{best}'? "))
                .unwrap_or_default();
            let available = if names.is_empty() {
                String::new()
            } else {
                format!("Available actions: {}. ", names.join(", "))
            };
            apperr::usage(
                format!("Unknown action: {id}"),
                format!("{suggestion}{available}Run `hycli {} --help`.", site.id),
            )
        })?;
        text.push_str(&format!(
            "{}\n{}\n\nUsage: hycli {} {} [OPTIONS]\n\nInputs:\n",
            op.title, op.description, site.id, id
        ));
        for field in &op.inputs {
            let default = field
                .default
                .as_ref()
                .map(|v| format!(", default: {v}"))
                .unwrap_or_default();
            text.push_str(&format!(
                "  --{} <{}>  {}{}{}\n",
                field.id,
                field.kind,
                if field.required {
                    "required; "
                } else {
                    "optional; "
                },
                field.label,
                default
            ));
            if !field.hint.is_empty() {
                text.push_str(&format!("      {}\n", field.hint));
            }
        }
        if op.inputs.is_empty() {
            text.push_str("  No inputs required.\n");
        }
        if op.accepts_account {
            text.push_str("\n  --hycli-account <ID>  Use a saved website account.\n");
        }
        text.push_str(&format!(
            "\nReturns: {}\nEffect: {}\nExample:\n  {}\n",
            op.output,
            op.effect.as_str(),
            sample_command(&site.id, op)
        ));
    } else {
        let guide = build_user_guide(site, None);
        text.push_str(&format!(
            "Usage: hycli {} <ACTION> [OPTIONS]\n\nWhat you can do:\n  {} read action(s), {} write action(s)\n\nActions:\n",
            site.id,
            guide.read_capabilities.len(),
            guide.write_capabilities.len()
        ));
        for op in &site.actions {
            let req_flags: Vec<String> = op
                .inputs
                .iter()
                .filter(|f| f.required)
                .map(|f| format!("--{} <{}>", f.id.replace('_', "-"), f.kind))
                .collect();
            let sig = if req_flags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", req_flags.join(" "))
            };
            text.push_str(&format!(
                "  {}  {} ({}){}\n",
                op.id,
                op.description,
                op.effect.as_str(),
                sig
            ));
        }
        if !guide.quick_start_commands.is_empty() {
            text.push_str("\nExamples:\n");
            for cmd in &guide.quick_start_commands {
                text.push_str(&format!("  {cmd}\n"));
            }
        }
    }
    text.push_str("\nUse an action's --help for inputs. Use `hycli describe SITE [ACTION]` for JSON metadata.\nFor exact input names or names reserved by the CLI, use `hycli run SITE ACTION --arg name=value`.\n");
    text.push_str(&format!("Missing functionality? Ask the AI: hycli request {} \"Describe the functionality to add\"\n", site.id));
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        policy::Effect,
        runtime::{ActionView, FieldView, SiteView},
    };

    #[test]
    fn builds_user_guide_with_categorized_capabilities_and_sample_commands() {
        let site = SiteView {
            id: "guild".into(),
            title: "Guild Hub".into(),
            url: "https://guild.example.com".into(),
            host: "guild.example.com".into(),
            summary: "Manage game channels and patch notes.".into(),
            status: "ready".into(),
            pinned: false,
            writes: true,
            account_id: String::new(),
            actions: vec![
                ActionView {
                    id: "list-channels".into(),
                    title: "List channels".into(),
                    description: "List all active guild channels.".into(),
                    output: "Channel list".into(),
                    effect: Effect::Read,
                    verified: true,
                    inputs: vec![],
                    credential_id: String::new(),
                    accepts_account: true,
                },
                ActionView {
                    id: "post-update".into(),
                    title: "Post update".into(),
                    description: "Publish a patch note to a channel.".into(),
                    output: "Posted message".into(),
                    effect: Effect::Write,
                    verified: false,
                    inputs: vec![FieldView {
                        id: "channel_id".into(),
                        label: "Channel ID".into(),
                        hint: "Target channel".into(),
                        kind: "string".into(),
                        required: true,
                        default: None,
                    }],
                    credential_id: String::new(),
                    accepts_account: true,
                },
            ],
            credentials: vec![],
            updated_at: String::new(),
            last_used: String::new(),
            translation_pending: false,
            image_url: String::new(),
        };
        let guide = build_user_guide(&site, None);
        assert_eq!(guide.read_capabilities.len(), 1);
        assert_eq!(guide.write_capabilities.len(), 1);
        assert!(guide.writes_enabled);
        assert_eq!(guide.enable_writes_command, "hycli writes guild allow");
        assert_eq!(
            guide.read_capabilities[0].example_command,
            "hycli guild list-channels"
        );
        assert_eq!(
            guide.write_capabilities[0].example_command,
            "hycli guild post-update --channel-id \"<id>\""
        );
        let briefing = format_user_briefing(&guide);
        assert!(briefing.contains("Read capabilities (run immediately):"));
        assert!(briefing.contains("Write capabilities (enabled — run directly):"));
        assert!(briefing.contains("hycli guild list-channels"));

        let mut locked = site.clone();
        locked.writes = false;
        let locked_guide = build_user_guide(&locked, None);
        assert!(!locked_guide.writes_enabled);
        assert!(
            locked_guide
                .agent_instruction
                .contains("hycli writes guild allow")
        );
        let locked_briefing = format_user_briefing(&locked_guide);
        assert!(
            locked_briefing.contains("ask user once to allow, then run `hycli writes guild allow`")
        );
    }

    #[test]
    fn closest_match_suggests_similar_flags_and_actions() {
        assert_eq!(
            closest_match("chanel-id", ["channel-id", "guild-id", "limit"]),
            Some("channel-id")
        );
        assert_eq!(
            closest_match("serch", ["search", "create-item", "read-account"]),
            Some("search")
        );
        assert_eq!(
            closest_match("completely-unrelated-xyz", ["search", "list"]),
            None
        );
    }
}

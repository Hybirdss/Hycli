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
            return Err(apperr::usage(
                format!("Unknown input: --{name}"),
                "Use the action's --help to list its inputs.",
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

pub fn help(site: &crate::runtime::SiteView, action: Option<&str>) -> AppResult<String> {
    let mut text = format!("{}\n{}\n\n", site.title, site.summary);
    if let Some(id) = action {
        let op = site.actions.iter().find(|op| op.id == id).ok_or_else(|| {
            apperr::usage(
                format!("Unknown action: {id}"),
                format!("hycli {} --help", site.id),
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
            "\nReturns: {}\nEffect: {}\n",
            op.output,
            op.effect.as_str()
        ));
    } else {
        text.push_str(&format!(
            "Usage: hycli {} <ACTION> [OPTIONS]\n\nActions:\n",
            site.id
        ));
        for op in &site.actions {
            text.push_str(&format!("  {}  {}\n", op.id, op.description));
        }
    }
    text.push_str("\nUse an action's --help for inputs. Use `hycli describe SITE [ACTION]` for JSON metadata.\nFor exact input names or names reserved by the CLI, use `hycli run SITE ACTION --arg name=value`.\n");
    text.push_str(&format!("Missing functionality? Ask the AI: hycli request {} \"Describe the functionality to add\"\n", site.id));
    Ok(text)
}

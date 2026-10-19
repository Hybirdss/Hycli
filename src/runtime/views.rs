use crate::{
    apperr::{AppError, AppResult},
    policy::{self, Effect},
};
use std::collections::BTreeMap;

use super::{
    ActionView, FieldView, Runtime, SiteView, WebsiteCredentialView, humanize, params,
    same_site_url,
};

impl Runtime {
    pub fn sites(&self, locale: &str) -> AppResult<Vec<SiteView>> {
        let data = self.state.read()?;
        let mut vault = self
            .vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        vault.reload()?;
        let mut out = vec![];
        for sp in self.specs()? {
            let m = data.sites.get(&sp.site.name).cloned().unwrap_or_default();
            let credentials: Vec<_> = sp
                .auth
                .iter()
                .filter(|auth| {
                    auth.kind == "header" && sp.operations.iter().any(|op| op.auth == auth.name)
                })
                .map(|auth| {
                    let key = auth
                        .value_from
                        .strip_prefix(&format!("store:{}/", sp.site.name))
                        .unwrap_or("");
                    WebsiteCredentialView {
                        id: auth.name.clone(),
                        label: humanize(&auth.name),
                        configured: vault
                            .get(&sp.site.name, key)
                            .is_some_and(|value| !value.is_empty()),
                    }
                })
                .collect();
            let hash = Self::spec_hash(&sp)?;
            let desc = m.descriptions.get(locale).filter(|d| d.hash == hash);
            let fallback = m.descriptions.get("en").filter(|d| d.hash == hash);
            let chosen = desc.or(fallback);
            let title = if !m.title.is_empty() {
                m.title.clone()
            } else {
                chosen
                    .map(|d| d.title.clone())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| {
                        if sp.site.title.is_empty() {
                            humanize(&sp.site.name)
                        } else {
                            sp.site.title.clone()
                        }
                    })
            };
            let actions = sp
                .operations
                .iter()
                .map(|op| {
                    let d = chosen.and_then(|s| s.actions.get(&op.name));
                    ActionView {
                        id: op.name.clone(),
                        accepts_account: same_site_url(sp.operation_base(op), sp.site.source_url())
                            .unwrap_or(false)
                            && !sp
                                .find_auth(&op.auth)
                                .is_some_and(|auth| auth.kind == "header"),
                        credential_id: sp
                            .find_auth(&op.auth)
                            .filter(|a| a.kind == "header")
                            .map(|a| a.name.clone())
                            .unwrap_or_default(),
                        title: d
                            .map(|d| d.title.clone())
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| humanize(&op.name)),
                        description: d
                            .map(|d| d.description.clone())
                            .unwrap_or_else(|| op.desc.clone()),
                        output: d.map(|d| d.output.clone()).unwrap_or_default(),
                        effect: policy::operation_effect(op, &BTreeMap::new()),
                        verified: m.verified.get(&op.name) == Some(&true),
                        inputs: params(op)
                            .into_iter()
                            .map(|(id, p)| {
                                let t = d.and_then(|d| d.inputs.get(id));
                                FieldView {
                                    id: id.clone(),
                                    label: t
                                        .map(|i| i.label.clone())
                                        .unwrap_or_else(|| humanize(id)),
                                    hint: t.map(|i| i.hint.clone()).unwrap_or_default(),
                                    kind: p.kind.clone(),
                                    required: p.required,
                                    default: p.default.clone(),
                                }
                            })
                            .collect(),
                    }
                })
                .collect();
            let running = data.jobs.iter().any(|j| {
                j.site_id == sp.site.name
                    && ["queued", "running"].contains(&j.status.as_str())
                    && j.kind == "prepare"
            });
            let has_verified_read = sp.operations.iter().any(|op| {
                m.verified.get(&op.name) == Some(&true)
                    && policy::operation_effect(op, &BTreeMap::new()) == Effect::Read
            });
            out.push(SiteView {
                id: sp.site.name.clone(),
                title,
                url: sp.site.source_url().into(),
                host: url::Url::parse(sp.site.source_url())
                    .ok()
                    .and_then(|u| u.host_str().map(str::to_owned))
                    .unwrap_or_default(),
                summary: chosen.map(|d| d.summary.clone()).unwrap_or_default(),
                status: if running {
                    "preparing"
                } else if (credentials.iter().any(|credential| !credential.configured)
                    && !has_verified_read)
                    || data
                        .accounts
                        .get(&m.account_id)
                        .is_some_and(|account| account.status == "expired")
                {
                    "needs_signin"
                } else if m.workflows_complete == Some(false)
                    || m.preparation
                        .as_ref()
                        .is_some_and(|report| report.verified_reads == 0)
                        && !has_verified_read
                {
                    "needs_review"
                } else {
                    "ready"
                }
                .into(),
                pinned: m.pinned,
                writes: m.writes,
                account_id: m.account_id,
                actions,
                credentials,
                updated_at: m.updated_at,
                last_used: m.last_used,
                translation_pending: desc.is_none(),
                image_url: if m.icon_type.is_empty() {
                    String::new()
                } else {
                    format!("/api/sites/{}/image", sp.site.name)
                },
            });
        }
        out.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(a.title.cmp(&b.title)));
        Ok(out)
    }
}

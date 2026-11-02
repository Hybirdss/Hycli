//! Explicit destination selection is local authority, not an account-id hint.
use super::{Runtime, origin, same_site_url};
use crate::{
    accounts::{AccountView, Credential},
    apperr::{AppError, AppResult},
    state::{AccountBinding, Data},
    util,
};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SelectionSnapshot {
    pub site_id: String,
    pub origin: String,
    pub revision: String,
    pub account_id: String,
    pub binding: Option<AccountBinding>,
}

fn denied() -> AppError {
    AppError::api("auth_required", 401)
}
fn context(view: &AccountView, credential: &Credential) -> AppResult<String> {
    credential.identity_proof_for(view, "binding-context")
}
pub(super) fn validate_binding(
    data: &Data,
    site: &str,
    url: &str,
    id: &str,
    credential: &Credential,
) -> AppResult<()> {
    let view = data.accounts.get(id).ok_or_else(denied)?;
    if view.site_id == site {
        if !same_site_url(&view.site_url, url)? {
            return Err(denied());
        }
        return Ok(());
    }
    let meta = data.sites.get(site).ok_or_else(denied)?;
    let binding = meta.account_binding.as_ref().ok_or_else(denied)?;
    if meta.account_selection_revision.is_empty()
        || meta.account_id != id
        || binding.account_id != id
        || binding.source_site_id != view.site_id
        || !credential.proves_identity(
            view,
            data.identity_generations
                .get(id)
                .map(String::as_str)
                .unwrap_or(""),
        )?
        || origin(url)? != binding.origin
        || origin(&view.site_url)? != binding.origin
        || origin(&credential.origin)? != binding.origin
        || context(view, credential)? != binding.context
    {
        return Err(denied());
    }
    Ok(())
}
impl Runtime {
    pub(crate) fn selection_lock(&self) -> AppResult<std::fs::File> {
        util::file_lock(&self.root.join("account-selection.lock"), false)
    }
    /// Called only by existing explicit user selection endpoints, never discovery/model tools.
    pub(crate) fn select_account(
        &self,
        site: &str,
        id: &str,
        requested_url: &str,
    ) -> AppResult<()> {
        self.select_account_checked(site, id, requested_url, false)
    }
    pub(crate) fn select_verified_account(
        &self,
        site: &str,
        id: &str,
        requested_url: &str,
    ) -> AppResult<()> {
        self.select_account_checked(site, id, requested_url, true)
    }
    fn select_account_checked(
        &self,
        site: &str,
        id: &str,
        requested_url: &str,
        require_verified: bool,
    ) -> AppResult<()> {
        let _guard = self.selection_lock()?;
        if require_verified && id.is_empty() {
            return Err(denied());
        }
        let target = origin(self.spec(site)?.site.source_url())?;
        if target != origin(requested_url)? {
            return Err(denied());
        }
        let credential = if id.is_empty() {
            None
        } else {
            Some(self.credential(id)?)
        };
        self.state.update(|data| {
            let binding = if let Some(credential) = &credential {
                let view = data.accounts.get(id).ok_or_else(denied)?;
                if (require_verified && (!view.verified_identity || view.status != "connected"))
                    || origin(&view.site_url)? != target
                    || origin(&credential.origin)? != target
                    || (view.site_id != site
                        && !credential.proves_identity(
                            view,
                            data.identity_generations
                                .get(id)
                                .map(String::as_str)
                                .unwrap_or(""),
                        )?)
                {
                    return Err(denied());
                }
                Some(AccountBinding {
                    account_id: id.into(),
                    source_site_id: view.site_id.clone(),
                    origin: target.clone(),
                    context: context(view, credential)?,
                })
            } else {
                None
            };
            let meta = data.sites.get_mut(site).ok_or_else(denied)?;
            meta.account_id = id.into();
            meta.account_binding = binding;
            meta.account_selection_revision = util::id();
            Ok(())
        })?;
        self.policy.invalidate(Some(site), None)?;
        self.changed();
        Ok(())
    }
    pub(crate) fn authorize_account_snapshot(
        &self,
        site: &str,
        url: &str,
        id: &str,
        credential: &Credential,
    ) -> AppResult<()> {
        validate_binding(&self.state.read()?, site, url, id, credential)
    }
    pub(crate) fn selected_credential(
        &self,
        site: &str,
        url: &str,
        id: &str,
    ) -> AppResult<Credential> {
        let _guard = self.selection_lock()?;
        let credential = self.credential(id)?;
        self.authorize_account_snapshot(site, url, id, &credential)?;
        Ok(credential)
    }
    pub(crate) fn ensure_credential_snapshot(
        &self,
        site: &str,
        url: &str,
        id: &str,
        snapshot: Option<&Credential>,
    ) -> AppResult<()> {
        if id.is_empty() {
            return Ok(());
        }
        let current = self.selected_credential(site, url, id)?;
        if serde_json::to_vec(&current)? != serde_json::to_vec(snapshot.ok_or_else(denied)?)? {
            return Err(denied());
        }
        Ok(())
    }
    pub(crate) fn selection_snapshot(
        &self,
        site: &str,
        url: &str,
    ) -> AppResult<Option<SelectionSnapshot>> {
        let _guard = self.selection_lock()?;
        let data = self.state.read()?;
        let Some(meta) = data
            .sites
            .get(site)
            .filter(|m| !m.account_selection_revision.is_empty())
        else {
            return Ok(None);
        };
        let snapshot = SelectionSnapshot {
            site_id: site.into(),
            origin: origin(url)?,
            revision: meta.account_selection_revision.clone(),
            account_id: meta.account_id.clone(),
            binding: meta.account_binding.clone(),
        };
        if !snapshot.account_id.is_empty() {
            let credential = self.credential(&snapshot.account_id)?;
            validate_binding(&data, site, url, &snapshot.account_id, &credential)?;
        }
        Ok(Some(snapshot))
    }
    pub(crate) fn preparation_selection_snapshot(
        &self,
        site: &str,
        url: &str,
    ) -> AppResult<SelectionSnapshot> {
        Ok(self
            .selection_snapshot(site, url)?
            .unwrap_or(SelectionSnapshot {
                site_id: site.into(),
                origin: origin(url)?,
                revision: String::new(),
                account_id: String::new(),
                binding: None,
            }))
    }
    pub(crate) fn validate_selection_data(
        &self,
        data: &Data,
        snapshot: &SelectionSnapshot,
        credential: Option<&Credential>,
    ) -> AppResult<()> {
        // No explicit choice at preparation start still carries a revision guard:
        // discovery may choose a source-owned account, but a later user choice wins.
        if snapshot.revision.is_empty() {
            if data
                .sites
                .get(&snapshot.site_id)
                .is_some_and(|m| !m.account_selection_revision.is_empty())
            {
                return Err(denied());
            }
            return Ok(());
        }
        let meta = data.sites.get(&snapshot.site_id).ok_or_else(denied)?;
        if meta.account_selection_revision != snapshot.revision
            || meta.account_id != snapshot.account_id
            || meta.account_binding != snapshot.binding
        {
            return Err(denied());
        }
        if !snapshot.account_id.is_empty() {
            validate_binding(
                data,
                &snapshot.site_id,
                &snapshot.origin,
                &snapshot.account_id,
                credential.ok_or_else(denied)?,
            )?;
        }
        Ok(())
    }
    pub(crate) fn ensure_job_credential_snapshot(
        &self,
        job: &crate::state::Job,
        id: &str,
        url: &str,
        expected: Option<&Credential>,
    ) -> AppResult<()> {
        if id.is_empty() {
            let _guard = self.selection_lock()?;
            if let Some(selection) = &job.account_selection {
                self.validate_selection_data(&self.state.read()?, selection, None)?;
            }
            return Ok(());
        }
        let current = self.job_credential(job, id, url)?;
        if serde_json::to_vec(&current)? != serde_json::to_vec(expected.ok_or_else(denied)?)? {
            return Err(denied());
        }
        Ok(())
    }
    pub(crate) fn job_credential(
        &self,
        job: &crate::state::Job,
        id: &str,
        url: &str,
    ) -> AppResult<Credential> {
        let _guard = self.selection_lock()?;
        let credential = self.credential(id)?;
        let data = self.state.read()?;
        if let Some(snapshot) = &job.account_selection {
            self.validate_selection_data(&data, snapshot, Some(&credential))?;
            if !snapshot.revision.is_empty() && id != snapshot.account_id {
                return Err(denied());
            }
        }
        validate_binding(&data, &job.site_id, url, id, &credential)?;
        Ok(credential)
    }
}

use crate::{
    accounts::{self, AccountView, Credential},
    apperr::{AppError, AppResult},
    spec::{Operation, Spec},
    util,
};
use std::collections::BTreeMap;

use super::{Runtime, origin, site_id_for};

impl Runtime {
    pub(crate) fn headers(
        &self,
        sp: &Spec,
        op: &Operation,
        url: &str,
        account: &str,
    ) -> AppResult<(Vec<(String, String)>, Vec<String>)> {
        let strategy = sp.find_auth(&op.auth);
        if strategy.is_some_and(|auth| auth.kind == "header") {
            let auth = strategy.unwrap();
            let scope = if auth.origin.is_empty() {
                &sp.site.base_url
            } else {
                &auth.origin
            };
            if origin(url)? != origin(scope)? {
                return Err(AppError::api("unsafe_target", 400));
            }
        }
        if strategy.is_some_and(|auth| auth.kind == "browser")
            && !self
                .state
                .account(account)?
                .is_some_and(|view| view.verified_identity && view.status == "connected")
        {
            return Err(AppError::api("auth_required", 401));
        }
        if strategy.is_some_and(|auth| auth.kind == "derived")
            && !self
                .state
                .account(account)?
                .is_some_and(|view| view.status == "connected")
        {
            return Err(AppError::api("auth_required", 401));
        }
        let mut vault = self
            .vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        vault.reload()?;
        let mut headers: BTreeMap<_, _> = sp
            .defaults
            .headers
            .iter()
            .chain(op.headers.iter())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut secrets = accounts::collect_redaction_secrets(&vault);
        let mut loaded_credential: Option<Credential> = None;
        if !account.is_empty() {
            let credential: Credential = serde_json::from_str(
                vault
                    .get("accounts", account)
                    .ok_or_else(|| AppError::api("auth_required", 401))?,
            )?;
            let session_headers = credential.request_headers(url)?;
            if session_headers.is_empty() && !strategy.is_some_and(|a| a.kind == "derived") {
                return Err(AppError::api("auth_required", 401));
            }
            secrets.extend(credential.redaction_values());
            headers.extend(session_headers);
            loaded_credential = Some(credential);
        }
        if let Some(auth) = strategy {
            if auth.kind == "header" {
                let path = auth
                    .value_from
                    .strip_prefix("store:")
                    .ok_or_else(|| AppError::api("data_invalid", 400))?;
                let (site, key) = path
                    .split_once('/')
                    .ok_or_else(|| AppError::api("data_invalid", 400))?;
                if let Some(secret) = vault.get(site, key) {
                    headers.retain(|key, _| !key.eq_ignore_ascii_case(&auth.header));
                    headers.insert(auth.header.clone(), format!("{}{}", auth.prefix, secret));
                } else {
                    return Err(AppError::api("auth_required", 401));
                }
            } else if auth.kind == "derived" {
                let credential = loaded_credential
                    .as_ref()
                    .ok_or_else(|| AppError::api("auth_required", 401))?;
                let derived = credential.resolve_derived_value(&auth.value_from, url)?;
                let full_val = format!("{}{derived}", auth.prefix);
                secrets.push(derived);
                secrets.push(full_val.clone());
                headers.retain(|key, _| !key.eq_ignore_ascii_case(&auth.header));
                headers.insert(auth.header.clone(), full_val);
            }
        }
        if let Some(body) = &op.body {
            headers.retain(|key, _| !key.eq_ignore_ascii_case("content-type"));
            headers.insert("content-type".into(), body.content_type().into());
        }
        Ok((headers.into_iter().collect(), secrets))
    }
    pub fn set_website_credential(
        &self,
        site: &str,
        name: &str,
        value: Option<&str>,
    ) -> AppResult<()> {
        let sp = self.spec(site)?;
        let auth = sp
            .find_auth(name)
            .filter(|auth| auth.kind == "header")
            .ok_or_else(|| AppError::api("not_found", 404))?;
        let key = auth
            .value_from
            .strip_prefix(&format!("store:{site}/"))
            .filter(|key| !key.is_empty())
            .ok_or_else(|| AppError::api("data_invalid", 400))?;
        let mut vault = self
            .vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        if let Some(value) = value {
            let value = value.trim();
            if value.is_empty() || value.len() > 16384 || value.contains(['\r', '\n']) {
                return Err(AppError::api("bad_request", 400));
            }
            vault.set(site, key, value)?;
        } else {
            vault.delete(site, key)?;
        }
        drop(vault);
        if value.is_some() {
            let origin = origin(if auth.origin.is_empty() {
                &sp.site.base_url
            } else {
                &auth.origin
            })?;
            self.policy.resume_auth(&format!("{origin}|"))?;
            for account in self
                .state
                .read()?
                .accounts
                .values()
                .filter(|account| account.site_id == site)
            {
                self.policy
                    .resume_auth(&format!("{origin}|{}", account.id))?;
            }
        }
        self.policy.invalidate(Some(site), None)?;
        self.changed();
        Ok(())
    }
    pub fn credential(&self, id: &str) -> AppResult<Credential> {
        let mut vault = self
            .vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        vault.reload()?;
        serde_json::from_str(
            vault
                .get("accounts", id)
                .ok_or_else(|| AppError::api("auth_required", 401))?,
        )
        .map_err(Into::into)
    }
    pub fn update_credential<R>(
        &self,
        id: &str,
        mutate: impl FnOnce(&mut Credential) -> AppResult<R>,
    ) -> AppResult<(Credential, R)> {
        let mut vault = self
            .vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        vault.reload()?;
        let mut credential: Credential = serde_json::from_str(
            vault
                .get("accounts", id)
                .ok_or_else(|| AppError::api("auth_required", 401))?,
        )?;
        let out = mutate(&mut credential)?;
        vault.set("accounts", id, &serde_json::to_string(&credential)?)?;
        Ok((credential, out))
    }
    pub fn connect_account(
        &self,
        site_id: &str,
        url: &str,
        cookies: Vec<accounts::Cookie>,
        browser: &str,
        profile: &str,
        identity_source: &str,
    ) -> AppResult<AccountView> {
        let url = crate::net::validate_url(url)?;
        let cookies = accounts::normalize(cookies, &url)?;
        let site_id = if site_id.is_empty() {
            site_id_for(&url)
        } else {
            if origin(self.spec(site_id)?.site.source_url())? != url.origin().ascii_serialization()
            {
                return Err(AppError::api("bad_url", 400));
            }
            site_id.into()
        };
        let credential = Credential {
            origin: url.origin().ascii_serialization(),
            cookies,
            identity_source: identity_source.into(),
            browser_session: String::new(),
            extension_origin: String::new(),
            browser_session_expires_ms: 0,
            source_profile: String::new(),
            local_storage: BTreeMap::new(),
            headers: BTreeMap::new(),
            recipe: None,
        };
        self.register_browser_credential(&site_id, credential, browser, profile)
    }
    pub(crate) fn register_browser_credential(
        &self,
        site_id: &str,
        credential: Credential,
        browser: &str,
        profile: &str,
    ) -> AppResult<AccountView> {
        let id = util::id();
        let site_id = site_id.to_string();
        let view = AccountView {
            id: id.clone(),
            site_id: site_id.clone(),
            site_url: credential.origin.clone(),
            label: String::new(),
            browser: browser.chars().take(80).collect(),
            profile: profile.chars().take(80).collect(),
            status: "connected".into(),
            checked_at: String::new(),
            cookie_count: credential.cookies.len(),
            created_at: util::now(),
            ..Default::default()
        };
        self.vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?
            .set("accounts", &id, &serde_json::to_string(&credential)?)?;
        self.state.update(|d| {
            d.accounts.insert(id.clone(), view.clone());
            let m = d.sites.entry(site_id).or_default();
            if m.account_id.is_empty() {
                m.account_id = id;
            }
            Ok(())
        })?;
        self.changed();
        Ok(view)
    }
    pub async fn refresh_account(&self, id: &str) -> AppResult<AccountView> {
        let mut view = self
            .state
            .read()?
            .accounts
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::api("not_found", 404))?;
        let (c, _) = self.update_credential(id, |c| {
            if let Some(recipe) = &c.recipe {
                c.headers.extend(recipe.resolve_headers(c)?);
            }
            Ok(())
        })?;
        self.policy.resume_auth(&format!("{}|{id}", c.origin))?;
        // Only a URL actually observed in this user's browser or a verified read operation.
        let source = if !c.identity_source.is_empty() {
            c.identity_source.clone()
        } else {
            self.state
                .read()?
                .sites
                .get(&view.site_id)
                .and_then(|m| m.identity_paths.first())
                .cloned()
                .unwrap_or_default()
        };
        if source.is_empty() {
            return Err(AppError::api("identity_unavailable", 422));
        }
        if !source.is_empty() {
            let parsed = crate::net::validate_url(&source)?;
            if parsed.origin().ascii_serialization() != c.origin
                || (c.recipe.is_none() && !accounts::identity_endpoint(&parsed))
            {
                return Err(AppError::api("data_invalid", 400));
            }
            let r = self.read_page(&source, id).await?;
            if r.successful() {
                let identity = if let Some(recipe) = &c.recipe {
                    recipe.identity_from_response(&r)
                } else {
                    r.json().as_ref().and_then(accounts::extract_identity)
                };
                if let Some(identity) = identity {
                    accounts::apply_identity(&mut view, identity);
                } else {
                    view.email.clear();
                    view.user_id.clear();
                    view.name.clear();
                    view.label.clear();
                    view.verified_identity = false;
                }
            } else if r.status == 401 || r.status == 403 {
                view.status = "expired".into();
                view.verified_identity = false;
            }
        }
        view.checked_at = util::now();
        self.state.update(|d| {
            d.accounts.insert(id.into(), view.clone());
            Ok(())
        })?;
        self.changed();
        if !view.verified_identity && view.status == "connected" {
            return Err(AppError::api("identity_unavailable", 422));
        }
        Ok(view)
    }
}

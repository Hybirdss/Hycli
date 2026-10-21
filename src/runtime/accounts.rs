use crate::{
    accounts::{self, AccountView, Credential, Identity},
    apperr::{AppError, AppResult},
    spec::{Operation, Spec},
    util,
};
use std::collections::BTreeMap;

use super::{Runtime, origin, site_id_for};

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum CandidateIdentityProbe {
    Capture,
    Recipe,
}

#[derive(Clone, Copy, PartialEq)]
enum IdentityProbe {
    SavedRefresh,
    Candidate(CandidateIdentityProbe),
}

impl IdentityProbe {
    fn accepts_origin(self, source: &url::Url, origin: &str) -> AppResult<bool> {
        Ok(if self == Self::Candidate(CandidateIdentityProbe::Recipe) {
            crate::net::same_site_origin(source, &crate::net::validate_url(origin)?)
        } else {
            source.origin().ascii_serialization() == origin
        })
    }
}

enum IdentityCheck {
    Verified {
        credential: Credential,
        identity: Identity,
    },
    SignedOut,
    Unproven,
}

impl Runtime {
    pub(crate) fn headers(
        &self,
        sp: &Spec,
        op: &Operation,
        url: &str,
        account: &str,
    ) -> AppResult<(Vec<(String, String)>, Vec<String>)> {
        let (headers, secrets, _) = self.headers_snapshot(sp, op, url, account)?;
        Ok((headers, secrets))
    }
    pub(crate) fn headers_snapshot(
        &self,
        sp: &Spec,
        op: &Operation,
        url: &str,
        account: &str,
    ) -> AppResult<(Vec<(String, String)>, Vec<String>, Option<Credential>)> {
        let _selection = self.selection_lock()?;
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
        let oauth_token = strategy
            .filter(|a| a.oauth_refresh.is_some())
            .map(|a| self.oauth_header(sp, a))
            .transpose()?;
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
            self.authorize_account_snapshot(&sp.site.name, url, account, &credential)?;
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
                if auth.oauth_refresh.is_some() {
                    let secret = oauth_token.ok_or_else(|| AppError::api("auth_required", 401))?;
                    headers.retain(|key, _| !key.eq_ignore_ascii_case(&auth.header));
                    headers.insert(auth.header.clone(), format!("{}{}", auth.prefix, secret));
                    secrets.push(secret);
                } else {
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
            // A multipart upload sets its own content-type (with the boundary) in
            // the network layer; do not force one here.
            if body.multipart.is_empty() {
                headers.retain(|key, _| !key.eq_ignore_ascii_case("content-type"));
                headers.insert("content-type".into(), body.content_type().into());
            }
        }
        Ok((headers.into_iter().collect(), secrets, loaded_credential))
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
        if auth.oauth_refresh.is_some() {
            return self.set_oauth_credential(&sp, auth, value);
        }
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
            vault.delete_website_credential(site, name, None, Some(key))?;
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
        let _selection = self.selection_lock()?;
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
        let before = serde_json::to_vec(&credential)?;
        let out = mutate(&mut credential)?;
        if serde_json::to_vec(&credential)? != before {
            credential.identity_proof.clear();
        }
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
            identity_proof: String::new(),
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
        let mut credential = credential;
        credential.identity_proof.clear();
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
            if m.account_id.is_empty() && m.account_selection_revision.is_empty() {
                m.account_id = id;
            }
            Ok(())
        })?;
        self.changed();
        Ok(view)
    }
    pub async fn refresh_account(&self, id: &str) -> AppResult<AccountView> {
        let data = self.state.read()?;
        let view = data
            .accounts
            .get(id)
            .ok_or_else(|| AppError::api("not_found", 404))?;
        let expected = self.credential(id)?;
        let mut candidate = expected.clone();
        let source = if !candidate.identity_source.is_empty() {
            candidate.identity_source.clone()
        } else {
            data.sites
                .get(&view.site_id)
                .and_then(|m| m.identity_paths.first())
                .cloned()
                .unwrap_or_default()
        };
        if source.is_empty() {
            return Err(AppError::api("identity_unavailable", 422));
        }
        candidate.identity_source = source;
        match self
            .check_identity(view, candidate, id, IdentityProbe::SavedRefresh)
            .await?
        {
            IdentityCheck::Verified {
                credential,
                identity,
            } => self.publish_verified_identity(id, &expected, credential, identity),
            IdentityCheck::SignedOut => {
                self.invalidate_identity_if_current(id, &expected, true)?;
                self.state
                    .account(id)?
                    .ok_or_else(|| AppError::api("not_found", 404))
            }
            IdentityCheck::Unproven => {
                self.invalidate_identity_if_current(id, &expected, false)?;
                Err(AppError::api("identity_unavailable", 422))
            }
        }
    }

    async fn check_identity(
        &self,
        view: &AccountView,
        mut candidate: Credential,
        scope: &str,
        probe: IdentityProbe,
    ) -> AppResult<IdentityCheck> {
        if let Some(recipe) = &candidate.recipe {
            candidate
                .headers
                .extend(recipe.resolve_headers(&candidate)?);
        }
        let source = &candidate.identity_source;
        let parsed = crate::net::validate_url(source)?;
        if !probe.accepts_origin(&parsed, &candidate.origin)?
            || view.site_url != candidate.origin
            || !crate::policy::safe_read_url(&parsed)
            || (candidate.recipe.is_none() && !accounts::identity_endpoint(&parsed))
        {
            return Err(AppError::api("data_invalid", 400));
        }
        self.policy.resume_auth(&format!(
            "{}|{scope}",
            parsed.origin().ascii_serialization()
        ))?;
        let request = crate::net::Request {
            url: source.clone(),
            method: "GET".into(),
            headers: candidate.request_headers(source)?,
            body: None,
            account_id: scope.into(),
            limit: if probe == IdentityProbe::Candidate(CandidateIdentityProbe::Recipe) {
                1024 * 1024
            } else {
                2 * 1024 * 1024
            },
            timeout: std::time::Duration::from_secs(
                if probe == IdentityProbe::Candidate(CandidateIdentityProbe::Recipe) {
                    25
                } else {
                    30
                },
            ),
            read_only: true,
            follow_redirects: false,
            tls_profile: "standard".into(),
            multipart: None,
        };
        let response = self.network.fetch(request.clone()).await?;
        if response.status == 401 {
            return Ok(IdentityCheck::SignedOut);
        }
        if !response.successful() {
            return Err(AppError::api("request_failed", 422));
        }
        let extract = |response: &crate::net::Response| {
            if let Some(recipe) = &candidate.recipe {
                recipe.identity_from_response(response)
            } else {
                response
                    .json()
                    .as_ref()
                    .and_then(accounts::extract_identity)
            }
        };
        let Some(identity) = extract(&response) else {
            return Ok(IdentityCheck::Unproven);
        };
        let baseline_scope = format!("{scope}:identity-baseline");
        self.policy.resume_auth(&format!(
            "{}|{baseline_scope}",
            parsed.origin().ascii_serialization()
        ))?;
        let anonymous = self
            .network
            .fetch(crate::net::Request {
                headers: vec![],
                account_id: baseline_scope,
                ..request
            })
            .await?;
        if !anonymous.successful() && anonymous.status != 401 && anonymous.status != 403 {
            return Err(AppError::api(
                if probe == IdentityProbe::SavedRefresh {
                    "identity_unavailable"
                } else {
                    "request_failed"
                },
                422,
            ));
        }
        if anonymous.successful() && extract(&anonymous).is_some()
            || !crate::session_recipe::identity_is_safe(&identity, &candidate)
        {
            return Ok(IdentityCheck::Unproven);
        }
        Ok(IdentityCheck::Verified {
            credential: candidate,
            identity,
        })
    }

    pub(crate) async fn verify_identity_candidate(
        &self,
        id: &str,
        expected: &Credential,
        candidate: Credential,
        probe: CandidateIdentityProbe,
    ) -> AppResult<AccountView> {
        let view = self
            .state
            .account(id)?
            .ok_or_else(|| AppError::api("not_found", 404))?;
        let scope = format!("{id}:identity-candidate");
        match self
            .check_identity(&view, candidate, &scope, IdentityProbe::Candidate(probe))
            .await?
        {
            IdentityCheck::Verified {
                credential,
                identity,
            } => {
                let origin = credential.origin.clone();
                let identity_origin = crate::net::validate_url(&credential.identity_source)?
                    .origin()
                    .ascii_serialization();
                let view = self.publish_verified_identity(id, expected, credential, identity)?;
                self.policy.resume_auth(&format!("{origin}|{id}"))?;
                if identity_origin != origin {
                    self.policy
                        .resume_auth(&format!("{identity_origin}|{id}"))?;
                }
                Ok(view)
            }
            IdentityCheck::SignedOut => Err(AppError::api("auth_required", 422)),
            IdentityCheck::Unproven => Err(AppError::api("identity_unavailable", 422)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CandidateIdentityProbe, IdentityProbe};

    #[test]
    fn identity_probe_preserves_each_callers_existing_origin_boundary() {
        let same = url::Url::parse("https://app.example.com/api/me").unwrap();
        let sibling = url::Url::parse("https://api.example.com/api/me").unwrap();
        let unrelated = url::Url::parse("https://unrelated.test/api/me").unwrap();
        for probe in [
            IdentityProbe::SavedRefresh,
            IdentityProbe::Candidate(CandidateIdentityProbe::Capture),
            IdentityProbe::Candidate(CandidateIdentityProbe::Recipe),
        ] {
            assert!(
                probe
                    .accepts_origin(&same, "https://app.example.com")
                    .unwrap()
            );
            assert!(
                !probe
                    .accepts_origin(&unrelated, "https://app.example.com")
                    .unwrap()
            );
            assert_eq!(
                probe
                    .accepts_origin(&sibling, "https://app.example.com")
                    .unwrap(),
                probe == IdentityProbe::Candidate(CandidateIdentityProbe::Recipe)
            );
        }
    }
}

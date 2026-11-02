use super::Runtime;
use crate::{
    apperr::{AppError, AppResult},
    net::Request,
    spec::{AuthStrategy, OAuthRefreshBinding, Operation, Spec},
    util,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantInput {
    kind: String,
    client_id: String,
    #[serde(default)]
    client_secret: String,
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    subject: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Ready,
    ExchangePending,
    VerificationPending,
    RecoveryRequired,
}

#[derive(Clone, Serialize, Deserialize)]
struct Grant {
    #[serde(default)]
    site: String,
    #[serde(default)]
    auth_name: String,
    owner: String,
    generation: u64,
    binding_hash: String,
    phase: Phase,
    input: GrantInput,
}

/// Public credential reference, without exposing the private grant or its storage key.
pub(crate) fn credential_ref(raw: &str) -> Option<String> {
    let grant: Grant = serde_json::from_str(raw).ok()?;
    if !crate::util::valid_id(&grant.site) || grant.auth_name.is_empty() {
        return None;
    }
    Some(format!("{}/{}", grant.site, grant.auth_name))
}

pub(crate) fn configured(store: &crate::store::Store, sp: &Spec, auth: &AuthStrategy) -> bool {
    let Some(binding) = auth.oauth_refresh.as_ref() else {
        return false;
    };
    store
        .get("@oauth", &binding.key(&sp.site.name))
        .and_then(|raw| serde_json::from_str::<Grant>(raw).ok())
        .is_some_and(|grant| {
            binding
                .hash(&sp.site.name, auth)
                .is_ok_and(|hash| hash == grant.binding_hash)
                && grant.phase != Phase::RecoveryRequired
        })
}

pub(crate) fn validate_operation(
    sp: &Spec,
    op: &Operation,
    inputs: &std::collections::BTreeMap<String, String>,
    account: &str,
) -> AppResult<()> {
    let Some(auth) = sp.find_auth(&op.auth).filter(|a| a.oauth_refresh.is_some()) else {
        return Ok(());
    };
    auth.oauth_refresh.as_ref().unwrap().check()?;
    if auth.kind != "header"
        || !auth.header.eq_ignore_ascii_case("authorization")
        || auth.prefix != "Bearer "
        || auth.origin.is_empty()
        || !auth.value_from.is_empty()
        || !account.is_empty()
        || op.effect != crate::policy::Effect::Read
        || crate::policy::operation_effect(op, inputs) != crate::policy::Effect::Read
        || !matches!(op.transport.as_str(), "" | "http")
        || !op.preflight.is_empty()
    {
        return Err(invalid());
    }
    Ok(())
}

pub(crate) fn redaction_values(value: &str) -> Vec<String> {
    serde_json::from_str::<Grant>(value)
        .map(|g| {
            vec![
                g.input.client_secret,
                g.input.access_token,
                g.input.refresh_token,
            ]
            .into_iter()
            .filter(|value| !value.is_empty())
            .collect()
        })
        .unwrap_or_default()
}

fn failure() -> AppError {
    AppError::api("oauth_recovery_required", 401)
}
fn invalid() -> AppError {
    AppError::api("oauth_configuration_invalid", 400)
}
fn clean(value: &str) -> bool {
    !value.is_empty() && value.len() <= 16384 && !value.chars().any(char::is_control)
}
fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
fn scope_set(values: &[String]) -> BTreeSet<&str> {
    values.iter().map(String::as_str).collect()
}

impl OAuthRefreshBinding {
    pub(crate) fn check(&self) -> AppResult<()> {
        if self.grant_ref.is_empty()
            || self.grant_ref.len() > 128
            || !self
                .grant_ref
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || !self.subject_pointer.starts_with('/')
            || !clean(&self.resource)
            || self.scopes.iter().any(|s| !clean(s) || s.contains(' '))
            || self.scopes.len() > 100
            || scope_set(&self.scopes).len() != self.scopes.len()
            || [
                &self.client_pointer,
                &self.resource_pointer,
                &self.scopes_pointer,
            ]
            .iter()
            .any(|p| !p.is_empty() && !p.starts_with('/'))
        {
            return Err(invalid());
        }
        for endpoint in [&self.token_endpoint, &self.identity_endpoint] {
            let parsed = crate::net::validate_url(endpoint).map_err(|_| invalid())?;
            if parsed.fragment().is_some()
                || parsed.query().is_some()
                || (parsed.scheme() != "https"
                    && !matches!(parsed.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")))
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
    fn hash(&self, site: &str, auth: &AuthStrategy) -> AppResult<String> {
        Ok(util::hash(&serde_json::to_vec(&(
            site,
            &auth.name,
            &auth.origin,
            &auth.value_from,
            self,
        ))?))
    }
    fn key(&self, site: &str) -> String {
        util::hash(format!("{site}/{}", self.grant_ref).as_bytes())
    }
}

impl Runtime {
    fn oauth_load(&self, key: &str) -> AppResult<(Grant, String)> {
        let mut store = self
            .vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?;
        store.reload()?;
        let raw = store.get("@oauth", key).ok_or_else(failure)?.to_owned();
        let grant = serde_json::from_str(&raw).map_err(|_| failure())?;
        Ok((grant, raw))
    }
    fn oauth_commit(
        &self,
        key: &str,
        expected: Option<&str>,
        grant: Option<&Grant>,
    ) -> AppResult<String> {
        let next = grant.map(serde_json::to_string).transpose()?;
        self.vault
            .lock()
            .map_err(|_| AppError::api("internal", 500))?
            .oauth_replace(key, expected, next.as_deref())?;
        Ok(next.unwrap_or_default())
    }
    fn oauth_lock(&self, key: &str) -> AppResult<std::fs::File> {
        util::file_lock(&self.root.join("oauth-locks").join(key), true)
    }
    async fn oauth_wait_lock(&self, key: &str) -> AppResult<std::fs::File> {
        let start = Instant::now();
        loop {
            match self.oauth_lock(key) {
                Ok(lock) => return Ok(lock),
                Err(error)
                    if error.public_code == "conflict"
                        && start.elapsed() < Duration::from_secs(35) =>
                {
                    tokio::time::sleep(Duration::from_millis(25)).await
                }
                Err(error) => return Err(error),
            }
        }
    }
    pub(crate) fn set_oauth_credential(
        &self,
        sp: &Spec,
        auth: &AuthStrategy,
        value: Option<&str>,
    ) -> AppResult<()> {
        let binding = auth.oauth_refresh.as_ref().ok_or_else(invalid)?;
        binding.check()?;
        if auth.name.is_empty()
            || auth.kind != "header"
            || !auth.header.eq_ignore_ascii_case("authorization")
            || auth.prefix != "Bearer "
            || auth.origin.is_empty()
            || !auth.value_from.is_empty()
        {
            return Err(invalid());
        }
        let key = binding.key(&sp.site.name);
        let _lock = self.oauth_lock(&key)?;
        let previous = {
            let mut vault = self
                .vault
                .lock()
                .map_err(|_| AppError::api("internal", 500))?;
            vault.reload()?;
            vault.get("@oauth", &key).map(str::to_owned)
        };
        let next = if let Some(value) = value {
            if value.len() > 65536 {
                return Err(invalid());
            }
            let input: GrantInput = serde_json::from_str(value).map_err(|_| invalid())?;
            if input.kind != "oauth_refresh"
                || !clean(&input.client_id)
                || !clean(&input.access_token)
                || !clean(&input.refresh_token)
                || !clean(&input.subject)
                || (!input.client_secret.is_empty() && !clean(&input.client_secret))
                || input.expires_at <= 0
            {
                return Err(invalid());
            }
            Some(Grant {
                site: sp.site.name.clone(),
                auth_name: auth.name.clone(),
                owner: util::id(),
                generation: 0,
                binding_hash: binding.hash(&sp.site.name, auth)?,
                phase: Phase::VerificationPending,
                input,
            })
        } else {
            None
        };
        if let Some(next) = next.as_ref() {
            self.oauth_commit(&key, previous.as_deref(), Some(next))?;
        } else {
            self.vault
                .lock()
                .map_err(|_| AppError::api("internal", 500))?
                .delete_website_credential(&sp.site.name, &auth.name, Some(&key), None)?;
        }
        self.policy.invalidate(Some(&sp.site.name), None)?;
        self.changed();
        Ok(())
    }
    pub(crate) fn oauth_header(&self, sp: &Spec, auth: &AuthStrategy) -> AppResult<String> {
        let binding = auth.oauth_refresh.as_ref().ok_or_else(invalid)?;
        binding.check()?;
        let (grant, _) = self.oauth_load(&binding.key(&sp.site.name))?;
        if grant.binding_hash != binding.hash(&sp.site.name, auth)?
            || matches!(grant.phase, Phase::RecoveryRequired)
        {
            return Err(failure());
        }
        Ok(grant.input.access_token)
    }
    pub(crate) fn oauth_credential_hash(
        &self,
        sp: &Spec,
        op: &Operation,
    ) -> AppResult<Option<String>> {
        let Some(binding) = sp
            .find_auth(&op.auth)
            .and_then(|a| a.oauth_refresh.as_ref())
        else {
            return Ok(None);
        };
        let (grant, _) = self.oauth_load(&binding.key(&sp.site.name))?;
        Ok(Some(util::hash(
            format!("{}/{}", grant.owner, grant.binding_hash).as_bytes(),
        )))
    }
    fn oauth_check_spec(&self, sp: &Spec, installed: bool) -> AppResult<()> {
        match self.spec(&sp.site.name) {
            Ok(current) if Self::spec_hash(&current)? != Self::spec_hash(sp)? => {
                Err(AppError::api("conflict", 409))
            }
            Err(error) if installed => Err(error),
            _ => Ok(()),
        }
    }
    fn oauth_check_current(&self, key: &str, expected: &str) -> AppResult<()> {
        if self.oauth_load(key)?.1 != expected {
            return Err(AppError::api("conflict", 409));
        }
        Ok(())
    }
    async fn oauth_verify(
        &self,
        key: &str,
        binding: &OAuthRefreshBinding,
        sp: &Spec,
        installed: bool,
        grant: &mut Grant,
        raw: &mut String,
    ) -> AppResult<()> {
        let response = tokio::time::timeout(
            Duration::from_secs(12),
            self.network.fetch_oauth_guarded(
                Request {
                    url: binding.identity_endpoint.clone(),
                    method: "GET".into(),
                    headers: vec![(
                        "authorization".into(),
                        format!("Bearer {}", grant.input.access_token),
                    )],
                    body: None,
                    account_id: format!("oauth-{key}"),
                    limit: 65536,
                    timeout: Duration::from_secs(10),
                    read_only: true,
                    follow_redirects: false,
                    multipart: None,
                    tls_profile: "hycli".into(),
                },
                None,
                &|| {
                    self.oauth_check_spec(sp, installed)?;
                    self.oauth_check_current(key, raw)
                },
                None,
            ),
        )
        .await
        .map_err(|_| AppError::api("oauth_verification_pending", 409))??;
        if !response.successful() {
            return Err(AppError::api("oauth_verification_pending", 409));
        }
        let identity: Value = serde_json::from_slice(&response.bytes)
            .map_err(|_| AppError::api("oauth_verification_pending", 409))?;
        let subject_matches = identity
            .pointer(&binding.subject_pointer)
            .and_then(Value::as_str)
            == Some(&grant.input.subject);
        let client_matches = binding.client_pointer.is_empty()
            || identity
                .pointer(&binding.client_pointer)
                .and_then(Value::as_str)
                == Some(&grant.input.client_id);
        let resource_matches = binding.resource_pointer.is_empty()
            || identity
                .pointer(&binding.resource_pointer)
                .and_then(Value::as_str)
                == Some(&binding.resource);
        let scopes_match = binding.scopes_pointer.is_empty()
            || identity
                .pointer(&binding.scopes_pointer)
                .and_then(Value::as_str)
                .is_some_and(|s| {
                    s.split_ascii_whitespace().collect::<BTreeSet<_>>()
                        == scope_set(&binding.scopes)
                });
        if !subject_matches || !client_matches || !resource_matches || !scopes_match {
            grant.phase = Phase::RecoveryRequired;
            *raw = self.oauth_commit(key, Some(raw), Some(grant))?;
            return Err(failure());
        }
        grant.phase = Phase::Ready;
        *raw = self.oauth_commit(key, Some(raw), Some(grant))?;
        Ok(())
    }
    async fn oauth_ready(
        &self,
        sp: &Spec,
        auth: &AuthStrategy,
        rejected: Option<(&str, u64)>,
        budget: &mut bool,
        expected_hash: &str,
        installed: bool,
    ) -> AppResult<(Grant, String)> {
        let binding = auth.oauth_refresh.as_ref().ok_or_else(invalid)?;
        binding.check()?;
        if auth.kind != "header"
            || !auth.header.eq_ignore_ascii_case("authorization")
            || auth.prefix != "Bearer "
            || auth.origin.is_empty()
            || !auth.value_from.is_empty()
        {
            return Err(invalid());
        }
        let key = binding.key(&sp.site.name);
        let _lock = self.oauth_wait_lock(&key).await?;
        let (mut grant, mut raw) = self.oauth_load(&key)?;
        self.oauth_check_spec(sp, installed)?;
        if util::hash(format!("{}/{}", grant.owner, grant.binding_hash).as_bytes()) != expected_hash
        {
            return Err(AppError::api("conflict", 409));
        }
        if grant.binding_hash != binding.hash(&sp.site.name, auth)?
            || matches!(
                grant.phase,
                Phase::ExchangePending | Phase::RecoveryRequired
            )
        {
            return Err(failure());
        }
        if grant.phase == Phase::VerificationPending && grant.input.expires_at > now() + 30 {
            self.oauth_verify(&key, binding, sp, installed, &mut grant, &mut raw)
                .await?;
        }
        let rejected_current = rejected.is_some_and(|(owner, generation)| {
            owner == grant.owner && generation == grant.generation
        });
        if rejected.is_some_and(|(owner, _)| owner != grant.owner) {
            return Err(failure());
        }
        if !rejected_current && grant.input.expires_at > now() + 30 {
            return Ok((grant, raw));
        }
        if !*budget {
            return Err(failure());
        }
        *budget = false;
        let before_exchange = grant.clone();
        grant.phase = Phase::ExchangePending;
        raw = self.oauth_commit(&key, Some(&raw), Some(&grant))?;
        let mut fields = vec![
            ("grant_type", "refresh_token"),
            ("refresh_token", grant.input.refresh_token.as_str()),
            ("client_id", grant.input.client_id.as_str()),
        ];
        if !grant.input.client_secret.is_empty() {
            fields.push(("client_secret", &grant.input.client_secret));
        }
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields)
            .finish()
            .into_bytes();
        let dispatched = std::sync::atomic::AtomicBool::new(false);
        let response = tokio::time::timeout(
            Duration::from_secs(12),
            self.network.fetch_oauth_guarded(
                Request {
                    url: binding.token_endpoint.clone(),
                    method: "POST".into(),
                    headers: vec![(
                        "content-type".into(),
                        "application/x-www-form-urlencoded".into(),
                    )],
                    body: Some(body),
                    account_id: format!("oauth-{key}"),
                    limit: 65536,
                    timeout: Duration::from_secs(10),
                    read_only: false,
                    follow_redirects: false,
                    multipart: None,
                    tls_profile: "hycli".into(),
                },
                None,
                &|| {
                    self.oauth_check_spec(sp, installed)?;
                    self.oauth_check_current(&key, &raw)
                },
                Some(&dispatched),
            ),
        )
        .await;
        if !dispatched.load(std::sync::atomic::Ordering::SeqCst) {
            self.oauth_commit(&key, Some(&raw), Some(&before_exchange))?;
            return Err(match response {
                Ok(Err(error)) => error,
                _ => AppError::api("timeout", 502),
            });
        }
        let token = response
            .ok()
            .and_then(Result::ok)
            .filter(|r| r.successful())
            .and_then(|r| serde_json::from_slice::<Value>(&r.bytes).ok());
        let Some(token) = token else {
            return Err(failure());
        };
        let access = token
            .get("access_token")
            .and_then(Value::as_str)
            .filter(|s| clean(s));
        let expiry = token
            .get("expires_in")
            .and_then(Value::as_i64)
            .filter(|v| *v > 0 && *v <= 31_536_000);
        let refresh = token
            .get("refresh_token")
            .map(|v| v.as_str().filter(|s| clean(s)));
        let valid_scope = token.get("scope").is_none_or(|v| {
            v.as_str().is_some_and(|s| {
                s.split_ascii_whitespace().collect::<BTreeSet<_>>() == scope_set(&binding.scopes)
            })
        });
        let pinned = token
            .get("client_id")
            .is_none_or(|v| v.as_str() == Some(&grant.input.client_id))
            && token
                .get("resource")
                .is_none_or(|v| v.as_str() == Some(&binding.resource));
        if access.is_none()
            || expiry.is_none()
            || refresh == Some(None)
            || token
                .get("token_type")
                .and_then(Value::as_str)
                .is_none_or(|s| !s.eq_ignore_ascii_case("bearer"))
            || !valid_scope
            || !pinned
        {
            grant.phase = Phase::RecoveryRequired;
            self.oauth_commit(&key, Some(&raw), Some(&grant))?;
            return Err(failure());
        }
        grant.input.access_token = access.unwrap().into();
        if let Some(Some(refresh)) = refresh {
            grant.input.refresh_token = refresh.into();
        }
        grant.input.expires_at = now() + expiry.unwrap();
        grant.generation = grant.generation.checked_add(1).ok_or_else(failure)?;
        grant.phase = Phase::VerificationPending;
        raw = self.oauth_commit(&key, Some(&raw), Some(&grant))?;
        self.oauth_verify(&key, binding, sp, installed, &mut grant, &mut raw)
            .await?;
        Ok((grant, raw))
    }
    pub(crate) async fn fetch_oauth_operation(
        &self,
        sp: &Spec,
        op: &Operation,
        mut request: Request,
        continuation: Option<(&str, &str)>,
        secrets: &mut Vec<String>,
        expected_hash: &str,
        receiver: Option<&crate::net::FileReceiver>,
    ) -> AppResult<crate::net::FileResponse> {
        let auth = sp.find_auth(&op.auth).ok_or_else(invalid)?;
        let binding = auth.oauth_refresh.as_ref().ok_or_else(invalid)?;
        if !request.read_only
            || !request.account_id.is_empty()
            || !matches!(op.transport.as_str(), "" | "http")
            || !op.preflight.is_empty()
        {
            return Err(invalid());
        }
        let key = binding.key(&sp.site.name);
        let installed = self.spec(&sp.site.name).is_ok();
        let mut budget = true;
        let (mut grant, mut raw) = self
            .oauth_ready(sp, auth, None, &mut budget, expected_hash, installed)
            .await?;
        for attempt in 0..2 {
            secrets.extend(redaction_values(&raw));
            request.account_id = format!("oauth-{key}");
            request
                .headers
                .retain(|(name, _)| !name.eq_ignore_ascii_case("authorization"));
            request.headers.push((
                "authorization".into(),
                format!("Bearer {}", grant.input.access_token),
            ));
            let response = self
                .network
                .fetch_oauth_resource_guarded(
                    request.clone(),
                    continuation,
                    &|| {
                        self.oauth_check_spec(sp, installed)?;
                        self.oauth_check_current(&key, &raw)
                    },
                    receiver,
                )
                .await?;
            {
                let _lock = self.oauth_wait_lock(&key).await?;
                self.oauth_check_spec(sp, installed)?;
                let (current, _) = self.oauth_load(&key)?;
                if current.owner != grant.owner || current.binding_hash != grant.binding_hash {
                    return Err(AppError::api("conflict", 409));
                }
                if current.phase != Phase::Ready {
                    return Err(failure());
                }
            }
            let expired = response.response.status == 401
                && response.response.headers.iter().any(|(name, value)| {
                    name.eq_ignore_ascii_case("www-authenticate")
                        && value.to_ascii_lowercase().starts_with("bearer ")
                        && value[7..].split(',').any(|part| {
                            part.trim().split_once('=').is_some_and(|(name, value)| {
                                name.trim().eq_ignore_ascii_case("error")
                                    && value.trim().trim_matches('"') == "invalid_token"
                            })
                        })
                });
            if !expired {
                return Ok(response);
            }
            drop(response);
            if attempt == 0 && budget {
                (grant, raw) = self
                    .oauth_ready(
                        sp,
                        auth,
                        Some((&grant.owner, grant.generation)),
                        &mut budget,
                        expected_hash,
                        installed,
                    )
                    .await?;
                continue;
            }
            let _lock = self.oauth_wait_lock(&key).await?;
            if self.oauth_load(&key)?.1 == raw {
                grant.phase = Phase::RecoveryRequired;
                self.oauth_commit(&key, Some(&raw), Some(&grant))?;
            }
            return Err(failure());
        }
        Err(failure())
    }
}

#[cfg(test)]
#[path = "oauth_tests.rs"]
mod tests;

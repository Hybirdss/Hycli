use super::*;
use base64::Engine;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Evidence {
    #[serde(default)]
    observation: Option<String>,
    #[serde(default)]
    har: Option<HarRef>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ArtifactEvidence {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    har: Option<HarRef>,
}
pub(super) struct ArtifactData {
    pub bytes: Vec<u8>,
    pub digest: String,
}
impl From<Loaded> for ArtifactData {
    fn from(value: Loaded) -> Self {
        Self {
            bytes: value.bytes,
            digest: value.digest,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HarRef {
    path: String,
    entry: usize,
}
#[derive(Serialize, Deserialize)]
struct ObservationRecord {
    account: String,
    binding: String,
    expires: i64,
    request: request::ReadRequest,
    json_response: bool,
}
pub(super) struct Loaded {
    pub request: request::ReadRequest,
    pub digest: String,
    pub bytes: Vec<u8>,
    pub json_response: bool,
    pub content_type: String,
    pub successful: bool,
}
impl Loaded {
    pub fn matches_request(
        &self,
        request: &request::ReadRequest,
        prepared: &request::Prepared,
    ) -> AppResult<()> {
        let observed = self.request.prepare()?;
        if observed.method != prepared.method
            || observed.url != prepared.url
            || observed.wire_body()? != prepared.wire_body()?
            || self.request.apollo_client_name != request.apollo_client_name
            || !self.json_response
        {
            return Err(unsupported());
        }
        Ok(())
    }
}
impl Runtime {
    /// Minted only by the existing observation command after proving its credential is still current.
    /// A reference is provenance, never a read effect or an authorization grant.
    pub(crate) fn save_account_read_observation(
        &self,
        id: &str,
        credential: &Credential,
        url: &str,
        json_response: bool,
    ) -> AppResult<Option<String>> {
        let data = self.state.read()?;
        let Some(account) = data.accounts.get(id) else {
            return Ok(None);
        };
        let generation = data
            .identity_generations
            .get(id)
            .map(String::as_str)
            .unwrap_or("");
        if !credential.proves_identity(account, generation)? {
            return Ok(None);
        }
        let binding = credential.identity_proof_for(account, generation)?;
        let record = ObservationRecord {
            account: id.into(),
            binding,
            expires: util::unix_ms() + 24 * 60 * 60 * 1000,
            request: request::ReadRequest {
                method: "GET".into(),
                url: url.into(),
                json: None,
                form: None,
                apollo_client_name: None,
            },
            json_response,
        };
        let mut vault = self.vault.lock().map_err(|_| invalid())?;
        vault.reload()?;
        let mut records: Vec<_> = vault
            .entries()
            .iter()
            .filter(|(key, _)| key.starts_with("@read-observation/"))
            .map(|(key, value)| {
                (
                    key.clone(),
                    serde_json::from_str::<ObservationRecord>(value)
                        .ok()
                        .map(|r| r.expires)
                        .unwrap_or(0),
                )
            })
            .collect();
        records.sort_by_key(|(_, expires)| *expires);
        let remove = records.len().saturating_sub(127);
        for (index, (key, expires)) in records.into_iter().enumerate() {
            if index < remove || expires <= util::unix_ms() {
                vault.delete(
                    "@read-observation",
                    key.trim_start_matches("@read-observation/"),
                )?;
            }
        }
        let reference = format!("ro_{}", util::id());
        vault.set(
            "@read-observation",
            &reference,
            &serde_json::to_string(&record)?,
        )?;
        Ok(Some(reference))
    }
    pub(super) fn read_evidence(
        &self,
        evidence: &Evidence,
        base: &Path,
        id: &str,
        snapshot: &Snapshot,
    ) -> AppResult<Loaded> {
        match (&evidence.observation, &evidence.har) {
            (Some(reference), None) => {
                if !reference.starts_with("ro_")
                    || reference.len() != 35
                    || !reference[3..].bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err(unsupported());
                }
                let mut vault = self.vault.lock().map_err(|_| invalid())?;
                vault.reload()?;
                let record: ObservationRecord = serde_json::from_str(
                    vault
                        .get("@read-observation", reference)
                        .ok_or_else(unsupported)?,
                )?;
                if record.account != id
                    || record.binding != snapshot.binding
                    || record.expires <= util::unix_ms()
                {
                    return Err(unsupported());
                }
                let digest = util::hash(&serde_json::to_vec(&record)?);
                Ok(Loaded {
                    request: record.request,
                    digest,
                    bytes: Vec::new(),
                    json_response: record.json_response,
                    content_type: String::new(),
                    successful: true,
                })
            }
            (None, Some(har)) => load_har(base, har),
            _ => Err(invalid()),
        }
    }
    pub(super) async fn read_artifact_evidence(
        &self,
        evidence: &ArtifactEvidence,
        base: &Path,
        id: &str,
        snapshot: &Snapshot,
        require_executable: bool,
    ) -> AppResult<ArtifactData> {
        // APQ is cryptographically bound to the actual wire document hash. Its
        // query document can come from a local build artifact, not a publication
        // convention that ordinary applications need not implement.
        if let (Some(path), None) = (&evidence.path, &evidence.har) {
            if require_executable || path.is_empty() {
                return Err(unsupported());
            }
            let bytes =
                util::read_bounded(&base.join(path), MAX_ARTIFACT).map_err(|_| invalid())?;
            return Ok(ArtifactData {
                digest: util::hash(&bytes),
                bytes,
            });
        }
        let (None, Some(har)) = (&evidence.path, &evidence.har) else {
            return Err(unsupported());
        };
        let mut loaded = load_har(base, har)?;
        let mut secrets = snapshot.credential.session_redaction_values();
        {
            let mut vault = self.vault.lock().map_err(|_| invalid())?;
            vault.reload()?;
            secrets.extend(collect_read_secrets(&vault));
        }
        screen_request_secrets(&loaded.request, &read_input_secrets(&secrets))?;
        if !require_executable {
            if loaded.bytes.is_empty() || loaded.bytes.len() > MAX_ARTIFACT {
                return Err(unsupported());
            }
            return Ok(loaded.into());
        }
        // Opaque IDs have no document hash. Only independently verified actual
        // first-party executable source can supply their static query contract.
        let url = net::validate_url(&loaded.request.url)?;
        let content_type = loaded.content_type.split(';').next().unwrap_or("").trim();
        if loaded.request.method != "GET"
            || loaded.request.json.is_some()
            || loaded.request.form.is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.origin() != net::validate_url(&snapshot.credential.origin)?.origin()
            || !policy::safe_session_read_url(&url)
            || !loaded.successful
            || loaded.bytes.is_empty()
            || loaded.bytes.len() > MAX_ARTIFACT
            || ![
                "application/json",
                "application/javascript",
                "text/javascript",
            ]
            .contains(&content_type)
        {
            return Err(unsupported());
        }
        let check = || self.ensure_account_read(id, &snapshot.binding);
        // Same-origin UGC/reflection is not deployed operation authority. Anchor the source
        // to an actual application asset declaration on the origin's root HTML document.
        let root_url = url::Url::parse(&format!("{}/", url.origin().ascii_serialization()))
            .map_err(|_| unsupported())?;
        let root_response = self
            .network
            .fetch_guarded(
                net::Request {
                    url: root_url.to_string(),
                    method: "GET".into(),
                    headers: vec![("accept".into(), "text/html".into())],
                    body: None,
                    account_id: String::new(),
                    limit: MAX_ARTIFACT,
                    timeout: Duration::from_secs(25),
                    read_only: true,
                    follow_redirects: false,
                    tls_profile: "standard".into(),
                    multipart: None,
                },
                None,
                &check,
            )
            .await?;
        check()?;
        if !root_response.successful()
            || root_response
                .header("content-type")
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                != "text/html"
        {
            return Err(unsupported());
        }
        // Enforced policies intersect; never inspect only the first header.
        if require_executable {
            for (_, policy) in root_response
                .headers
                .iter()
                .filter(|(name, _)| name.eq_ignore_ascii_case("content-security-policy"))
            {
                deployment::allows_policy(&root_url, &url, policy)?;
            }
        }
        deployment::references(
            &root_url,
            &url,
            content_type,
            &root_response.bytes,
            require_executable,
        )?;
        loaded.digest.push_str(&util::hash(&serde_json::to_vec(&(
            root_url.as_str(),
            url.as_str(),
            content_type,
        ))?));
        let response = self
            .network
            .fetch_guarded(
                net::Request {
                    url: url.to_string(),
                    method: "GET".into(),
                    headers: vec![("accept".into(), content_type.into())],
                    body: None,
                    account_id: String::new(),
                    limit: MAX_ARTIFACT,
                    timeout: Duration::from_secs(25),
                    read_only: true,
                    follow_redirects: false,
                    tls_profile: "standard".into(),
                    multipart: None,
                },
                None,
                &check,
            )
            .await?;
        check()?;
        if !response.successful()
            || response.bytes != loaded.bytes
            || response
                .header("content-type")
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                != content_type
        {
            return Err(unsupported());
        }
        Ok(loaded.into())
    }
}
fn load_har(base: &Path, reference: &HarRef) -> AppResult<Loaded> {
    if reference.path.is_empty() {
        return Err(invalid());
    }
    let bytes =
        util::read_bounded(&base.join(&reference.path), 8 * 1024 * 1024).map_err(|_| invalid())?;
    let har = strict_json::parse(&bytes)?;
    let entry = har
        .pointer("/log/entries")
        .and_then(Value::as_array)
        .and_then(|entries| entries.get(reference.entry))
        .ok_or_else(unsupported)?;
    let raw = entry
        .get("request")
        .and_then(Value::as_object)
        .ok_or_else(unsupported)?;
    let method = raw
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(unsupported)?
        .to_string();
    let url = raw
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(unsupported)?
        .to_string();
    let parsed = net::validate_url(&url)?;
    // HAR queryString may be omitted, but if present it cannot contradict the URL or repeat a field.
    if let Some(parameters) = raw.get("queryString") {
        let pairs = parameters.as_array().ok_or_else(invalid)?;
        let mut expected = BTreeMap::new();
        for pair in pairs {
            let key = pair
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            let value = pair
                .get("value")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            if expected
                .insert(key.to_string(), value.to_string())
                .is_some()
            {
                return Err(invalid());
            }
        }
        let mut actual = BTreeMap::new();
        for (key, value) in parsed.query_pairs() {
            if actual
                .insert(key.into_owned(), value.into_owned())
                .is_some()
            {
                return Err(invalid());
            }
        }
        if expected != actual {
            return Err(unsupported());
        }
    }
    let mut json_body = None;
    let mut form = None;
    if let Some(post) = raw.get("postData") {
        let mime = post
            .get("mimeType")
            .and_then(Value::as_str)
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("");
        let text = post.get("text").and_then(Value::as_str);
        let mut parameters = None;
        if let Some(params) = post.get("params") {
            let mut fields = BTreeMap::new();
            for pair in params.as_array().ok_or_else(invalid)? {
                if pair.get("fileName").is_some() {
                    return Err(unsupported());
                }
                let key = pair
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(invalid)?;
                let value = pair
                    .get("value")
                    .and_then(Value::as_str)
                    .ok_or_else(invalid)?;
                if fields.insert(key.to_string(), value.to_string()).is_some() {
                    return Err(invalid());
                }
            }
            if !fields.is_empty() {
                parameters = Some(fields);
            }
        }
        match mime {
            "application/json" => {
                if parameters.is_some() {
                    return Err(unsupported());
                }
                json_body = Some(strict_json::parse(
                    text.ok_or_else(unsupported)?.as_bytes(),
                )?);
            }
            "application/x-www-form-urlencoded" => {
                let mut fields = BTreeMap::new();
                if let Some(text) = text {
                    for (key, value) in url::form_urlencoded::parse(text.as_bytes()) {
                        if fields
                            .insert(key.into_owned(), value.into_owned())
                            .is_some()
                        {
                            return Err(invalid());
                        }
                    }
                    if parameters.as_ref().is_some_and(|params| params != &fields) {
                        return Err(unsupported());
                    }
                } else {
                    fields = parameters.ok_or_else(unsupported)?;
                }
                form = Some(fields);
            }
            _ => return Err(unsupported()),
        }
    }
    let mut client = None;
    if let Some(headers) = raw.get("headers").and_then(Value::as_array) {
        for header in headers {
            let name = header.get("name").and_then(Value::as_str).unwrap_or("");
            // Authentication values are deliberately neither imported nor compared.
            if name.eq_ignore_ascii_case("apollographql-client-name") {
                if client.is_some() {
                    return Err(unsupported());
                }
                client = Some(
                    header
                        .get("value")
                        .and_then(Value::as_str)
                        .ok_or_else(invalid)?
                        .to_string(),
                );
            }
            if [
                "x-http-method-override",
                "x-method-override",
                "x-http-method",
                "x-original-url",
                "x-rewrite-url",
            ]
            .contains(&name.to_ascii_lowercase().as_str())
            {
                return Err(unsupported());
            }
        }
    }
    let response = entry.get("response").ok_or_else(unsupported)?;
    let status = response
        .get("status")
        .and_then(Value::as_u64)
        .ok_or_else(unsupported)?;
    let content = response.get("content").ok_or_else(unsupported)?;
    let content_type = content
        .get("mimeType")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let text = content.get("text").and_then(Value::as_str).unwrap_or("");
    let response_bytes = match content.get("encoding").and_then(Value::as_str) {
        None | Some("") => text.as_bytes().to_vec(),
        Some("base64") => base64::engine::general_purpose::STANDARD
            .decode(text)
            .map_err(|_| invalid())?,
        _ => return Err(unsupported()),
    };
    let mime = content_type.split(';').next().unwrap_or("").trim();
    let request = request::ReadRequest {
        method,
        url,
        json: json_body,
        form,
        apollo_client_name: client,
    };
    // Scope capabilities to the exact imported request and source response, never to raw auth headers.
    let digest = util::hash(&serde_json::to_vec(&(
        &request,
        &content_type,
        status,
        util::hash(&response_bytes),
    ))?);
    Ok(Loaded {
        request,
        digest,
        bytes: response_bytes,
        json_response: mime == "application/json" || mime.ends_with("+json"),
        content_type,
        successful: (200..300).contains(&status),
    })
}

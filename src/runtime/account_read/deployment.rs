//! A caller's HAR cannot establish deployment authority. Only an explicit
//! executable/fetch asset edge from the independently fetched origin root can.
//! HTML link semantics: https://html.spec.whatwg.org/dev/links.html
//! Script semantics: https://html.spec.whatwg.org/dev/scripting.html
use std::collections::BTreeMap;

use scraper::{ElementRef, Html, Selector};
use url::Url;

use crate::apperr::{AppError, AppResult};

const MAX_HTML: usize = 1024 * 1024;
const MAX_REFERENCES: usize = 256;
const MAX_ANCESTORS: usize = 64;
const HTML_NAMESPACE: &str = "http://www.w3.org/1999/xhtml";

fn unsupported() -> AppError {
    AppError::api("unsupported_evidence", 422)
}

/// No fetches, script evaluation, guessed filenames, or caller-selected root.
/// The caller must fetch `root` anonymously, without following redirects, and
/// enforce a successful HTML response before passing its bytes here.
pub(super) fn references(
    root: &Url,
    artifact: &Url,
    mime: &str,
    html: &[u8],
    require_executable: bool,
) -> AppResult<()> {
    if html.len() > MAX_HTML
        || !static_url(root, root)
        || root.path() != "/"
        || !static_url(root, artifact)
    {
        return Err(unsupported());
    }
    let mime = ascii_trim(mime.split(';').next().unwrap_or(""));
    let javascript = matches!(mime, "application/javascript" | "text/javascript");
    let json = mime == "application/json";
    if (!javascript && !json) || (require_executable && !javascript) {
        return Err(unsupported());
    }
    let html = std::str::from_utf8(html).map_err(|_| unsupported())?;
    let document = Html::parse_document(html);
    let selector = Selector::parse("base[href], script[src], link[href], meta[http-equiv]")
        .map_err(|_| unsupported())?;
    let candidates: Vec<_> = document
        .select(&selector)
        .take(MAX_REFERENCES + 1)
        .collect();
    if candidates.len() > MAX_REFERENCES {
        return Err(unsupported());
    }
    let mut base = root.clone();
    let mut saw_base = false;
    let mut matched = false;
    for element in candidates {
        if !active_html(element)? {
            continue;
        }
        let name = element.value().name();
        if name == "meta" {
            if require_executable
                && element
                    .attr("http-equiv")
                    .is_some_and(|value| value.eq_ignore_ascii_case("Content-Security-Policy"))
            {
                allows_policy(
                    root,
                    artifact,
                    element.attr("content").ok_or_else(unsupported)?,
                )?;
            }
            continue;
        }
        if name == "base" {
            if saw_base {
                return Err(unsupported());
            }
            saw_base = true;
            base = resolve(root, root, element.attr("href").ok_or_else(unsupported)?)?;
            // A CSP base-uri restriction can make the browser ignore a base.
            // For executable authority, honoring or ignoring it must resolve
            // identically. APQ's document hash does not require execution.
            if require_executable && base != *root {
                return Err(unsupported());
            }
            continue;
        }
        let source = match name {
            "script"
                if javascript
                    && executable_script(element)
                    && (!require_executable
                        || ["nomodule", "event", "for"]
                            .into_iter()
                            .all(|name| element.attr(name).is_none())) =>
            {
                element.attr("src")
            }
            "link" if !require_executable => {
                let relations: Vec<_> = element
                    .attr("rel")
                    .unwrap_or("")
                    .split_ascii_whitespace()
                    .map(str::to_ascii_lowercase)
                    .collect();
                let module = javascript
                    && relations.iter().any(|rel| rel == "modulepreload")
                    && element
                        .attr("as")
                        .is_none_or(|kind| kind.eq_ignore_ascii_case("script"))
                    && element.attr("type").is_none_or(|kind| {
                        matches!(
                            ascii_trim(kind).to_ascii_lowercase().as_str(),
                            "" | "application/javascript"
                                | "text/javascript"
                                | "application/ecmascript"
                                | "text/ecmascript"
                        )
                    });
                let fetch = json
                    && relations.iter().any(|rel| rel == "preload")
                    && element
                        .attr("as")
                        .is_some_and(|kind| kind.eq_ignore_ascii_case("fetch"))
                    && element.attr("type").is_some_and(|kind| {
                        ascii_trim(kind).eq_ignore_ascii_case("application/json")
                    });
                if module || fetch {
                    element.attr("href")
                } else {
                    None
                }
            }
            _ => None,
        };
        let Some(source) = source.filter(|source| !ascii_trim(source).is_empty()) else {
            continue;
        };
        // SRI cannot be checked with only a URL and HTML. Do not silently treat
        // a resource whose integrity constraint may fail as deployed code.
        if element.attr("integrity").is_some() {
            continue;
        }
        // An unrelated CDN asset does not invalidate the root, but cannot prove
        // this exact-origin target. Invalid/queryful references never qualify.
        if resolve(root, &base, source).is_ok_and(|target| target == *artifact) {
            matched = true;
        }
    }
    if matched { Ok(()) } else { Err(unsupported()) }
}

/// A small CSP proof, not a browser interpreter. Each enforced header/meta
/// policy must independently allow the script. CSP3 directive fallback and
/// source matching: https://www.w3.org/TR/CSP3/#directive-script-src-elem
pub(super) fn allows_policy(root: &Url, artifact: &Url, policy: &str) -> AppResult<()> {
    if !static_url(root, root) || root.path() != "/" || !static_url(root, artifact)
        || policy.len() > 16 * 1024
        || !policy.is_ascii()
        || policy.bytes().any(|byte| byte.is_ascii_control() && !matches!(byte, b'\t' | b'\n' | b'\r' | b'\x0c'))
        // A comma represents multiple policies, not another source expression.
        || policy.contains(',')
    {
        return Err(unsupported());
    }
    let mut directives = BTreeMap::new();
    for directive in policy.split(';') {
        let mut tokens = directive.split_ascii_whitespace();
        let Some(name) = tokens.next() else {
            continue;
        };
        if !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(unsupported());
        }
        let values: Vec<_> = tokens.take(129).collect();
        if values.len() > 128
            || directives.len() >= 64
            || directives
                .insert(name.to_ascii_lowercase(), values)
                .is_some()
        {
            return Err(unsupported());
        }
    }
    if let Some(values) = directives.get("sandbox") {
        // An opaque sandbox origin also changes module fetch/CORS semantics.
        // Both capabilities are needed for this bounded execution proof.
        if !["allow-scripts", "allow-same-origin"]
            .into_iter()
            .all(|required| {
                values
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(required))
            })
        {
            return Err(unsupported());
        }
    }
    // These may change resource identity or add an integrity requirement that
    // this helper cannot establish. Do not infer execution from a source list.
    if directives.contains_key("require-sri-for")
        || (root.scheme() == "http" && directives.contains_key("upgrade-insecure-requests"))
    {
        return Err(unsupported());
    }
    let effective = ["script-src-elem", "script-src", "default-src"]
        .into_iter()
        .find_map(|name| directives.get(name));
    let Some(sources) = effective else {
        return Ok(());
    };
    if sources.iter().any(|source| {
        source.eq_ignore_ascii_case("'strict-dynamic'") || source.eq_ignore_ascii_case("'none'")
    }) {
        return Err(unsupported());
    }
    if sources.iter().any(|source| {
        *source == "'self'"
            || (explicit_csp_host(root)
                && (*source == root.origin().ascii_serialization() || *source == artifact.as_str()))
    }) {
        Ok(())
    } else {
        Err(unsupported())
    }
}

// CSP host-source grammar is narrower than URL host parsing. In particular,
// an arbitrary IP address, IPv6, or underscore hostname does not become an
// allowed script source merely because Url accepts it. 'self' is independent.
fn explicit_csp_host(root: &Url) -> bool {
    match root.host() {
        Some(url::Host::Domain(host)) => host.split('.').all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        }),
        Some(url::Host::Ipv4(address)) => address.octets() == [127, 0, 0, 1],
        _ => false,
    }
}

// HTML/CSP whitespace is ASCII; Unicode trimming can turn inert markup into
// purported executable authority (for example NBSP before a script type).
fn ascii_trim(value: &str) -> &str {
    value.trim_matches(|ch| matches!(ch, ' ' | '\t' | '\n' | '\r' | '\x0c'))
}

fn static_url(root: &Url, target: &Url) -> bool {
    matches!(target.scheme(), "http" | "https")
        && target.host_str().is_some()
        && target.username().is_empty()
        && target.password().is_none()
        && target.query().is_none()
        && target.fragment().is_none()
        && target.origin() == root.origin()
        && target.as_str().len() <= 8192
}

fn resolve(root: &Url, base: &Url, reference: &str) -> AppResult<Url> {
    if reference.len() > 8192 || reference.chars().any(char::is_control) || reference.contains('\\')
    {
        return Err(unsupported());
    }
    let target = base
        .join(ascii_trim(reference))
        .map_err(|_| unsupported())?;
    if static_url(root, &target) {
        Ok(target)
    } else {
        Err(unsupported())
    }
}

fn active_html(element: ElementRef<'_>) -> AppResult<bool> {
    if element.value().name.ns.as_ref() != HTML_NAMESPACE {
        return Ok(false);
    }
    for (index, ancestor) in element.ancestors().enumerate() {
        if index >= MAX_ANCESTORS {
            return Err(unsupported());
        }
        if let Some(ancestor) = ElementRef::wrap(ancestor) {
            if ancestor.value().name.ns.as_ref() != HTML_NAMESPACE
                || matches!(ancestor.value().name(), "template" | "noscript")
            {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn executable_script(element: ElementRef<'_>) -> bool {
    match element.attr("type") {
        Some(kind) => {
            ascii_trim(kind).is_empty()
                || ascii_trim(kind) == "module"
                || matches!(
                    ascii_trim(kind).to_ascii_lowercase().as_str(),
                    "application/javascript"
                        | "text/javascript"
                        | "application/ecmascript"
                        | "text/ecmascript"
                )
        }
        None => element.attr("language").is_none_or(|language| {
            ascii_trim(language).is_empty()
                || matches!(
                    language.to_ascii_lowercase().as_str(),
                    "javascript" | "ecmascript"
                )
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(path: &str, mime: &str, html: &str) -> AppResult<()> {
        let root = Url::parse("https://app.example.test/").unwrap();
        references(
            &root,
            &root.join(path).unwrap(),
            mime,
            html.as_bytes(),
            false,
        )
    }

    #[test]
    fn accepts_only_explicit_executable_or_fetch_assets() {
        for html in [
            "<script src='/build/query.js'></script>",
            "<script type='module' src='/build/query.js'></script>",
            "<script type='text/javascript' src='/build/query.js'></script>",
            "<link rel='modulepreload' href='/build/query.js'>",
            "<link rel='MODULEPRELOAD' as='script' href='/build/query.js'>",
        ] {
            assert!(check("/build/query.js", "application/javascript", html).is_ok());
        }
        for path in ["/build/apollo.json", "/build/relay.json"] {
            let html =
                format!("<link rel='preload' as='fetch' type='application/json' href='{path}'>");
            assert!(check(path, "application/json", &html).is_ok());
        }
    }

    #[test]
    fn rejects_reflected_unlinked_and_ordinary_navigation_sources() {
        for html in [
            "<html><head></head><body>No deployment edge</body></html>",
            "<a href='/upload.json'>User-uploaded manifest</a>",
            "<link rel='manifest' href='/upload.json'>",
            "<link rel='prefetch' href='/upload.json'>",
            "<script type='application/json' src='/upload.json'></script>",
            "<link rel='preload' as='image' type='application/json' href='/upload.json'>",
            "<link rel='preload' as='fetch' href='/upload.json'>",
            "<link rel='preload' as='fetch' type='application/json' href='/other.json'>",
        ] {
            assert!(check("/upload.json", "application/json", html).is_err());
        }
        assert!(check("/echo?payload=manifest", "application/json",
            "<link rel='preload' as='fetch' type='application/json' href='/echo?payload=manifest'>").is_err());
        assert!(
            check(
                "/query.js#selector",
                "application/javascript",
                "<script src='/query.js#selector'></script>"
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_inert_non_javascript_and_integrity_constrained_references() {
        for html in [
            "<!-- <script src='/query.js'></script> -->",
            "<template><script src='/query.js'></script></template>",
            "<noscript><script src='/query.js'></script></noscript>",
            "<script type='application/json' src='/query.js'></script>",
            "<script type='importmap' src='/query.js'></script>",
            "<script language='vbscript' src='/query.js'></script>",
            "<svg><script src='/query.js'></script></svg>",
            "<script src='/query.js' integrity='sha256-unverified'></script>",
            "<link rel='modulepreload' as='style' href='/query.js'>",
        ] {
            assert!(
                check("/query.js", "application/javascript", html).is_err(),
                "accepted {html}"
            );
        }
        assert!(check("/query.json", "application/json",
            "<template><link rel='preload' as='fetch' type='application/json' href='/query.json'></template>").is_err());
    }

    #[test]
    fn opaque_relay_requires_a_live_script_not_a_preload_or_blocked_edge() {
        let root = Url::parse("https://app.example.test/").unwrap();
        let artifact = root.join("/query.js").unwrap();
        for html in [
            "<link rel='modulepreload' href='/query.js'>",
            "<script nomodule src='/query.js'></script>",
            "<script type='\u{a0}text/javascript' src='/query.js'></script>",
            "<script src='\u{a0}/query.js'></script>",
            "<base href='/uploads/'><script src='../query.js'></script>",
            "<script integrity='sha256-unverified' src='/query.js'></script>",
            "<meta http-equiv='Content-Security-Policy' content=\"script-src 'none'\"><script src='/query.js'></script>",
            "<meta http-equiv='cOnTeNt-SeCuRiTy-PoLiCy' content=\"default-src 'none'\"><script src='/query.js'></script>",
        ] {
            assert!(
                references(
                    &root,
                    &artifact,
                    "application/javascript",
                    html.as_bytes(),
                    true
                )
                .is_err(),
                "accepted {html}"
            );
        }
        for html in [
            "<script src='/query.js'></script>",
            "<script type='module' src='/query.js'></script>",
            "<base href='/'><script src='query.js'></script>",
            "<meta http-equiv='Content-Security-Policy' content=\"script-src 'self'; base-uri 'none'\"><base href='/'><script src='query.js'></script>",
            "<meta http-equiv='Content-Security-Policy' content=\"script-src 'self'\"><script src='/query.js'></script>",
            "<meta http-equiv='Content-Security-Policy-Report-Only' content=\"script-src 'none'\"><script src='/query.js'></script>",
        ] {
            assert!(
                references(
                    &root,
                    &artifact,
                    "application/javascript",
                    html.as_bytes(),
                    true
                )
                .is_ok(),
                "rejected {html}"
            );
        }
        assert!(
            references(
                &root,
                &root.join("/query.json").unwrap(),
                "application/json",
                b"<link rel='preload' as='fetch' type='application/json' href='/query.json'>",
                true
            )
            .is_err()
        );
    }

    #[test]
    fn csp_proves_simple_sources_and_checks_effective_directive_precedence() {
        let root = Url::parse("https://app.example.test/").unwrap();
        let artifact = root.join("/query.js").unwrap();
        for policy in [
            "",
            "img-src 'none'",
            "script-src 'self'",
            "default-src 'self'",
            "default-src 'none'; script-src 'self'",
            "script-src 'none'; script-src-elem 'self'",
            "script-src https://app.example.test",
            "script-src https://app.example.test/query.js",
            "script-src 'self'; base-uri 'self'",
            "script-src 'self'; base-uri 'none'",
            "script-src 'self'; sandbox allow-scripts allow-same-origin",
        ] {
            assert!(
                allows_policy(&root, &artifact, policy).is_ok(),
                "rejected {policy}"
            );
        }
        for policy in [
            "script-src 'none'",
            "default-src 'none'",
            "script-src",
            "script-src 'self'; script-src-elem 'none'",
            "script-src 'self' 'strict-dynamic'",
            "script-src 'nonce-abc'",
            "script-src https://foreign.test",
            "script-src 'self'; sandbox",
            "script-src 'self'; sandbox allow-scripts",
            "script-src 'self'; sandbox allow-same-origin",
            "script-src 'self'; script-src 'none'",
            "script-src 'self', script-src 'none'",
            "default-src 'none'; script-src\u{a0}'self'",
            "script-src 'self'; require-sri-for script",
        ] {
            assert!(
                allows_policy(&root, &artifact, policy).is_err(),
                "accepted {policy}"
            );
        }
        assert!(allows_policy(&root, &artifact, &" ".repeat(16 * 1024 + 1)).is_err());
        let both = "<meta http-equiv='Content-Security-Policy' content=\"script-src 'self'\"><meta http-equiv='Content-Security-Policy' content=\"script-src 'none'\"><script src='/query.js'></script>";
        assert!(
            references(
                &root,
                &artifact,
                "application/javascript",
                both.as_bytes(),
                true
            )
            .is_err()
        );
        let base = "<meta http-equiv='Content-Security-Policy' content=\"script-src 'self'; base-uri 'none'\"><base href='/uploads/'><script src='query.js'></script>";
        assert!(
            references(
                &root,
                &root.join("/uploads/query.js").unwrap(),
                "application/javascript",
                base.as_bytes(),
                true
            )
            .is_err()
        );
    }

    #[test]
    fn resolves_a_single_same_origin_base_in_document_order() {
        assert!(
            check(
                "/assets/query.js",
                "application/javascript",
                "<base href='/assets/'><script src='query.js'></script>"
            )
            .is_ok()
        );
        assert!(
            check(
                "/query.js",
                "application/javascript",
                "<script src='query.js'></script><base href='/assets/'>"
            )
            .is_ok()
        );
        assert!(
            check(
                "/assets/query.js",
                "application/javascript",
                "<script src='query.js'></script><base href='/assets/'>"
            )
            .is_err()
        );
        assert!(check("/query.js", "application/javascript",
            "<template><base href='https://evil.test/'></template><script src='/query.js'></script>").is_ok());
    }

    #[test]
    fn rejects_foreign_queryful_and_ambiguous_bases() {
        for html in [
            "<base href='https://evil.test/'><script src='/query.js'></script>",
            "<base href='/?x=y'><script src='/query.js'></script>",
            "<base href='/#x'><script src='/query.js'></script>",
            "<base href='/'><base href='/assets/'><script src='/query.js'></script>",
            "<script src='https://evil.test/query.js'></script>",
            "<script src='http://app.example.test/query.js'></script>",
            "<script src='https://app.example.test:444/query.js'></script>",
        ] {
            assert!(check("/query.js", "application/javascript", html).is_err());
        }
        // Unrelated CDN resources are normal and cannot prove the target.
        assert!(check("/query.js", "application/javascript",
            "<script src='https://cdn.test/library.js'></script><script src='/query.js'></script>").is_ok());
    }

    #[test]
    fn requires_the_canonical_origin_root_and_bounded_inputs() {
        let artifact = Url::parse("https://app.example.test/query.js").unwrap();
        for root in [
            "https://app.example.test/user-page",
            "https://app.example.test/?payload=manifest",
            "https://app.example.test/#x",
        ] {
            assert!(
                references(
                    &Url::parse(root).unwrap(),
                    &artifact,
                    "application/javascript",
                    b"<script src='/query.js'></script>",
                    false
                )
                .is_err()
            );
        }
        let root = Url::parse("https://app.example.test/").unwrap();
        assert!(
            references(
                &root,
                &artifact,
                "application/javascript",
                &vec![b' '; MAX_HTML + 1],
                false
            )
            .is_err()
        );
        let many = format!(
            "{}<script src='/query.js'></script>",
            "<link href='/other'>".repeat(MAX_REFERENCES)
        );
        assert!(
            references(
                &root,
                &artifact,
                "application/javascript",
                many.as_bytes(),
                false
            )
            .is_err()
        );
        assert!(references(&root, &artifact, "application/javascript", b"\xff", false).is_err());
        let deep = format!(
            "{}<script src='/query.js'></script>{}",
            "<div>".repeat(MAX_ANCESTORS),
            "</div>".repeat(MAX_ANCESTORS)
        );
        assert!(
            references(
                &root,
                &artifact,
                "application/javascript",
                deep.as_bytes(),
                false
            )
            .is_err()
        );
    }

    #[test]
    fn deployment_parser_fits_one_mib_stack() {
        std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(|| {
                assert!(
                    check(
                        "/query.js",
                        "application/javascript",
                        "<script src='/query.js'></script>"
                    )
                    .is_ok()
                );
                let deep = format!(
                    "{}<script src='/query.js'></script>{}",
                    "<div>".repeat(MAX_ANCESTORS),
                    "</div>".repeat(MAX_ANCESTORS)
                );
                assert!(check("/query.js", "application/javascript", &deep).is_err());
                let very_deep = format!(
                    "{}<script src='/query.js'></script>{}",
                    "<div>".repeat(1024),
                    "</div>".repeat(1024)
                );
                assert!(check("/query.js", "application/javascript", &very_deep).is_err());
                let suffix = "--><script src='/query.js'></script>";
                let bounded = format!("<!--{}{}", " ".repeat(MAX_HTML - 4 - suffix.len()), suffix);
                assert_eq!(bounded.len(), MAX_HTML);
                assert!(check("/query.js", "application/javascript", &bounded).is_ok());
                assert!(check("/query.js", "application/javascript", &(bounded + " ")).is_err());
            })
            .unwrap()
            .join()
            .unwrap();
    }
}

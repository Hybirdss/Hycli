use std::collections::BTreeSet;
use url::Url;

use super::routes::{documented_routes, route_matches};

/// Exact published server addresses; origin changes never imply a credential grant.
pub fn api_bases(base: &Url, text: &str) -> BTreeSet<String> {
    let mut servers = BTreeSet::new();
    if let Some(document) = api_schema(text) {
        if document.get("openapi").is_some() {
            let mut nodes = vec![&document];
            if let Some(paths) = document.get("paths").and_then(|v| v.as_object()) {
                for path in paths.values() {
                    nodes.push(path);
                    if let Some(operations) = path.as_object() {
                        nodes.extend(operations.values().filter(|v| v.is_object()));
                    }
                }
            }
            let mut root_has_server = false;
            for (index, node) in nodes.into_iter().enumerate() {
                for server in node
                    .get("servers")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                {
                    if let Some(raw) = server.get("url").and_then(|v| v.as_str()) {
                        let mut raw = raw.to_string();
                        if let Some(variables) = server.get("variables").and_then(|v| v.as_object())
                        {
                            for (name, variable) in variables {
                                if let Some(default) =
                                    variable.get("default").and_then(|v| v.as_str())
                                {
                                    raw = raw.replace(&format!("{{{name}}}"), default);
                                }
                            }
                        }
                        if !raw.contains(['{', '}']) {
                            if let Ok(url) = base.join(&raw) {
                                if safe_server(&url) {
                                    servers.insert(url.to_string().trim_end_matches('/').into());
                                    root_has_server |= index == 0;
                                }
                            }
                        }
                    }
                }
            }
            if !root_has_server
                && document
                    .get("servers")
                    .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty))
            {
                servers.insert(base.origin().ascii_serialization());
            }
        }
        if document.get("swagger").is_some() {
            let prefix = document
                .get("basePath")
                .and_then(|v| v.as_str())
                .unwrap_or("/");
            let schemes: Vec<_> = document
                .get("schemes")
                .and_then(|v| v.as_array())
                .map(|values| values.iter().filter_map(|v| v.as_str()).collect())
                .filter(|values: &Vec<&str>| !values.is_empty())
                .unwrap_or_else(|| vec![base.scheme()]);
            for scheme in schemes {
                let origin = document
                    .get("host")
                    .and_then(|v| v.as_str())
                    .map(|host| format!("{scheme}://{host}"))
                    .unwrap_or_else(|| {
                        let mut url = base.clone();
                        let _ = url.set_scheme(scheme);
                        url.origin().ascii_serialization()
                    });
                if let Ok(url) = Url::parse(&format!("{origin}{prefix}")) {
                    if safe_server(&url) {
                        servers.insert(url.to_string().trim_end_matches('/').into());
                    }
                }
            }
        }
    }
    let visible = crate::runtime::visible_text(text);
    let pattern = regex::Regex::new(r#"(?i)(?:base\s+url|api\s+(?:base\s+)?(?:url|endpoint|server))\s*[:=]?\s*(https?://[^\s<>"'`]+)"#).unwrap();
    for matched in pattern.captures_iter(&visible) {
        if let Ok(url) = Url::parse(matched[1].trim_end_matches(['.', ','])) {
            if url.query().is_none()
                && url.fragment().is_none()
                && url.username().is_empty()
                && url.password().is_none()
            {
                servers.insert(url.to_string().trim_end_matches('/').into());
            }
        }
    }
    // A published absolute request example also establishes its server origin,
    // even when the documentation does not use the literal words "Base URL".
    let routes = documented_routes(&visible);
    for matched in regex::Regex::new(r#"https?://[^\s<>"'`\\)\]]+"#)
        .unwrap()
        .find_iter(&visible)
    {
        if let Ok(url) = Url::parse(matched.as_str().trim_end_matches(['.', ','])) {
            if url.username().is_empty()
                && url.password().is_none()
                && url.path() != "/"
                && routes
                    .iter()
                    .any(|(_, path)| route_matches(url.path(), path, &BTreeSet::new()))
            {
                servers.insert(url.origin().ascii_serialization());
            }
        }
    }
    servers
}

pub(super) fn safe_server(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.username().is_empty()
        && url.password().is_none()
}

/// OpenAPI can be published as JSON or YAML. Never interpret an ordinary page as a schema.
pub fn api_schema(text: &str) -> Option<serde_json::Value> {
    let document = serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .or_else(|| {
            if regex::Regex::new(r"(?m)^(?:openapi|swagger)\s*:")
                .unwrap()
                .is_match(text)
            {
                serde_yaml::from_str::<serde_json::Value>(text).ok()
            } else {
                None
            }
        })?;
    if document.get("openapi").is_some() || document.get("swagger").is_some() {
        Some(document)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;

    #[test]
    fn yaml_servers_resolve_defaults_variables_and_operation_overrides() {
        let source = Url::parse("https://docs.example.test:8443/reference/openapi.yaml").unwrap();
        let text = "openapi: 3.0.4\nservers:\n  - url: https://{tenant}.example.test/{version}\n    variables:\n      tenant: {default: api}\n      version: {default: v2}\npaths:\n  /items:\n    get:\n      servers:\n        - url: ../special\n";
        assert!(api_schema(text).is_some());
        let bases = api_bases(&source, text);
        assert!(bases.contains("https://api.example.test/v2"));
        assert!(bases.contains("https://docs.example.test:8443/special"));
        assert_eq!(
            api_bases(&source, "openapi: 3.0.4\npaths: {}"),
            BTreeSet::from(["https://docs.example.test:8443".into()])
        );
        assert_eq!(
            api_bases(&source, "swagger: '2.0'\nbasePath: /v1\npaths: {}"),
            BTreeSet::from(["https://docs.example.test:8443/v1".into()])
        );
        assert!(
            api_bases(
                &source,
                r#"{"openapi":"3.0.4","servers":[{"url":"https://user:secret@api.example.test"}]}"#
            )
            .is_empty()
        );
        assert!(api_schema("<html>openapi: is not a schema</html>").is_none());
    }
    #[test]
    fn api_servers_are_published_evidence_with_independent_origins() {
        let source = Url::parse("https://docs.example.net/openapi.json").unwrap();
        let bases = api_bases(
            &source,
            r#"{"openapi":"3.1.0","servers":[{"url":"https://api.example.com/v2"},{"url":"/local-api"}]}"#,
        );
        assert!(bases.contains("https://api.example.com/v2"));
        assert!(bases.contains("https://docs.example.net/local-api"));
        assert!(
            api_bases(&source, "<p>Base URL https://api.example.com/v3</p>")
                .contains("https://api.example.com/v3")
        );
        assert!(api_bases(&source, "<a href='https://unrelated.example.com'>Link</a>").is_empty());
        let examples = "GET /search/widgets\ncurl -H 'Accept: application/json' https://api.example.com/search/widgets?q=sample\nSee https://unrelated.example.net/elsewhere";
        let bases = api_bases(&source, examples);
        assert_eq!(bases, BTreeSet::from(["https://api.example.com".into()]));
        assert!(route_matches(
            "/teams/demo/items/123",
            "/teams/{team}/items/{item}",
            &BTreeSet::new()
        ));
        assert!(!route_matches(
            "/teams/demo/items/123/delete",
            "/teams/{team}/items/{item}",
            &BTreeSet::new()
        ));
    }
    #[test]
    fn follows_published_docs_across_hosts_and_markdown_indices() {
        let home = Url::parse("https://example.com/").unwrap();
        let links = published_links(
            &home,
            r#"<a href="https://docs.example.net/reference#start">Docs</a><a href="/logout">exit</a><script src="/assets/app.js"></script>"#,
        );
        assert!(links.contains("https://docs.example.net/reference"));
        assert!(!links.iter().any(|url| url.contains("logout")));
        let invalid = published_links(
            &home,
            r#"<a href='/resources/:type(.:format)'>Template</a><a href='/teams/{team}'>Template</a>"#,
        );
        assert!(invalid.is_empty());
        assert!(
            priority("https://docs.example.net/")
                > priority("https://example.com/resources/articles")
        );
        let docs = Url::parse("https://docs.example.net/llms.txt").unwrap();
        let index = published_links(
            &docs,
            "[Users](https://docs.example.net/resources/users.md)\n[Auth](/reference/authentication.md)",
        );
        assert_eq!(index.len(), 2);
        assert_eq!(
            ranked(links.iter())[0],
            "https://docs.example.net/reference"
        );
    }
    #[test]
    fn document_admission_excludes_resources_across_all_extractors() {
        let home = Url::parse("https://example.com/").unwrap();
        let text = r##"<link href='/icon?version=2#icon' rel='shortcut ICON'>
            <img src='/image?id=1'><link rel='stylesheet' href='/styles?version=2'>
            <a href='/favicon.SVG?version=2'>Icon</a>
            <a href='/photo.webp?id=2'>Photo</a>
            <script src='/assets/app.js'></script>
            <iframe src='/EntryList.page?blogId=demo'></iframe>
            <a href='/docs/reference.json?version=2'>API</a>
            <a href='/docs/manual.pdf'>Manual</a>
            <a href='/docs/format.svg.html'>HTML</a>
            [Icon](/icon?version=2) '/styles?version=2' '/image?id=1'
            '/assets/font.woff2?version=2' '/images/logo.png#brand'
            '/docs/reference.yaml' '/docs/reference.md' '/llms.txt'"##;
        assert_eq!(
            published_links(&home, text),
            BTreeSet::from([
                "https://example.com/EntryList.page?blogId=demo".into(),
                "https://example.com/docs/reference.json?version=2".into(),
                "https://example.com/docs/manual.pdf".into(),
                "https://example.com/docs/format.svg.html".into(),
                "https://example.com/docs/reference.yaml".into(),
                "https://example.com/docs/reference.md".into(),
                "https://example.com/llms.txt".into(),
            ])
        );
        assert_eq!(
            crate::discovery::bundle_urls(&home, text),
            vec!["https://example.com/assets/app.js"]
        );
        assert!(header_links(&home, "</favicon.svg?version=2>; rel=alternate").is_empty());
    }

    #[test]
    fn resource_entities_and_header_relations_do_not_become_documents() {
        let home = Url::parse("https://example.com/").unwrap();
        assert_eq!(
            published_links(
                &home,
                r#"<link rel='icon' href='/icon?version=2&#38;id=1'>
            <img src='/&#105;mage'><a href='/docs?version=2&#x26;id=1'>Document</a>
            [Manual](/manual?version=2&amp;id=1)"#
            ),
            BTreeSet::from([
                "https://example.com/docs?version=2&id=1".into(),
                "https://example.com/manual?version=2&id=1".into(),
            ])
        );
        assert_eq!(
            header_links(
                &home,
                r#"</icon>; rel="alternate icon",
            </image>; rel=icon; title="alternate image",
            </style>; rel=alternate; as=style,
            </reference>; REL="DESCRIBEDBY"; title="API, reference",
            </not-doc>; rel=not-alternate,
            </duplicate>; rel=alternate; rel=icon"#
            ),
            BTreeSet::from(["https://example.com/reference".into()])
        );
    }

    #[test]
    fn query_addressed_pages_remain_discoverable_without_credentials_or_mutations() {
        let home = Url::parse("https://blog.example.com/").unwrap();
        let links = published_links(
            &home,
            r##"<iframe id="mainFrame" src="/EntryList.page?blogId=demo&amp;categoryNo=0#posts"></iframe>
                <a href="/EntryView.page?blogId=demo&amp;logNo=123">Read entry</a>
                <a href="https://m.blog.example.com/EntryView.page?blogId=demo&amp;logNo=123">Mobile entry</a>
                <a href="/entries?entry_id=record-1&amp;page=2&amp;limit=20">Catalogue</a>
                <a href="/read?access_token=private">Secret</a>
                <a href="/read?sessionId=private">Session</a>
                <a href="/read?password=private">Password</a>
                <a href="/read?api_key=private">Key</a>
                <a href="/read?key=sk-abcdefghijklmnop0123456789">Opaque secret value</a>
                <a href="/entry?action=delete&amp;id=123">Delete</a>
                <a href="/entry?op=publish&amp;id=123">Publish</a>
                <a href="/logout?next=/">Sign out</a>
                <a href="http://blog.example.com/read?id=123">Downgrade</a>
                <a href="https://user:password@blog.example.com/read?id=123">User info</a>"##,
        );
        assert_eq!(
            links,
            BTreeSet::from([
                "https://blog.example.com/entries?entry_id=record-1&page=2&limit=20".into(),
                "https://blog.example.com/EntryList.page?blogId=demo&categoryNo=0".into(),
                "https://blog.example.com/EntryView.page?blogId=demo&logNo=123".into(),
                "https://m.blog.example.com/EntryView.page?blogId=demo&logNo=123".into(),
            ])
        );
    }

    #[test]
    fn query_navigation_excludes_unknown_commands_and_opaque_credentials() {
        let home = Url::parse("https://example.com/").unwrap();
        let unsafe_targets = [
            "/?do=delete&id=123",
            "/oauth/callback?code=opaqueAuthorizationCode&state=opaqueState",
            "/reference?sid=opaqueSessionId",
            "/reference?key=opaqueApiKey",
            "/graphql?query=mutation%20%7BdeleteAccount%7D",
            "/reference?authId=opaque",
            "/reference?tokenId=opaque",
            "/reference?sessionNo=opaque",
            "/reference?accessKeyId=opaque",
            "/reference?credentialId=opaque",
            "/reference?id=delete",
            "/reference?id=123&unknown=opaque",
            "/reference?id=123&id=456",
            "/reference?page=delete",
        ];
        for target in unsafe_targets {
            assert!(
                published_links(&home, &format!("<iframe src='{target}'></iframe>")).is_empty(),
                "published {target}"
            );
            assert!(
                header_links(&home, &format!("<{target}>; rel=describedby")).is_empty(),
                "header {target}"
            );
        }
    }

    #[test]
    fn published_header_documents_keep_safe_query_parameters() {
        let home = Url::parse("https://example.com/").unwrap();
        let links = header_links(
            &home,
            r#"</reference?version=2>; rel="describedby", </reference?token=private>; rel="describedby", </reference?action=delete>; rel="alternate", <http://example.com/reference?version=2>; rel="describedby", <https://user:password@example.com/reference?version=2>; rel="describedby", </reference?key=sk-abcdefghijklmnop0123456789>; rel="describedby""#,
        );
        assert_eq!(
            links,
            BTreeSet::from(["https://example.com/reference?version=2".into()])
        );
    }

    #[test]
    fn reference_routes_keep_methods_and_parameter_evidence() {
        let routes = documented_routes(
            "## Current user % GET /users/@me\n## Messages % GET /channels/{channel.id}/messages\n## Send % POST /channels/{channel.id}/messages",
        );
        assert!(routes.contains(&("GET".into(), "/users/@me".into())));
        assert!(routes.contains(&("POST".into(), "/channels/{channel.id}/messages".into())));
        assert!(!routes.contains(&("DELETE".into(), "/users/@me".into())));
        let base = Url::parse("https://example.com").unwrap();
        let prefixes = api_prefixes(
            &base,
            "Base https://example.com/api Version URL https://example.com/api/v{version_number} Version Status Default 10 Available 9 Available 8 Deprecated",
        );
        assert!(route_matches("/api/v10/users/@me", "/users/@me", &prefixes));
        assert!(!prefixes.contains("/api/v8"));
    }
    #[test]
    fn documentation_components_preserve_verbs_and_linked_parameters() {
        let text = crate::runtime::visible_text(
            r##"<Endpoint method="GET">/current-context</Endpoint>
<ApiRoute className="route" method='PATCH'>/teams/[\{team.id\}](/reference/team)/items/[\{item.id\}](/reference/item)</ApiRoute>"##,
        );
        let routes = documented_routes(&text);
        assert!(routes.contains(&("GET".into(), "/current-context".into())));
        assert!(routes.contains(&("PATCH".into(), "/teams/{team.id}/items/{item.id}".into())));
        assert_eq!(routes.len(), 2);
        let wrapped = crate::runtime::visible_text(
            r#"<span>get</span><span><wbr/>/teams<wbr/>/{team}<wbr/>/items</span><span>post</span><span><wbr>/teams<wbr>/{team}<wbr>/items</span>"#,
        );
        let routes = documented_routes(&wrapped);
        assert_eq!(
            routes,
            BTreeSet::from([
                ("GET".into(), "/teams/{team}/items".into()),
                ("POST".into(), "/teams/{team}/items".into())
            ])
        );
    }
    #[test]
    fn bundle_routes_and_urls_handle_multibyte_utf8_remix_preload_and_priority_sorting() {
        let korean_js = format!(
            "{}\"/api/v1/topics\"{}; new URL(\"/api/trending-posts?limit=20\", window.location.origin); {{\"urlTemplate\":\"https://www.daangn.com/kr/search/?q={{search_string}}\"}}",
            "가나다라마바사아자차카타파하을를이히".repeat(4),
            "한글텍스트경계테스트입니다".repeat(4)
        );
        let routes = bundle_routes(&korean_js);
        assert!(routes.contains(&("GET".into(), "/api/v1/topics".into())));
        assert!(routes.contains(&("GET".into(), "/api/trending-posts".into())));
        assert!(routes.contains(&("GET".into(), "/kr/search".into())));

        let base = Url::parse("https://www.daangn.com/kr/").unwrap();
        let html = r#"
            <script src="/_next/static/chunks/polyfills-abc.js"></script>
            <script src="https://assets.velcdn.com/_next/static/chunks/app/(list)/page-123.js"></script>
            <link rel="modulepreload" href="/_remix/entry.client-xyz.js" />
            <script type="module">import * as route0 from "/_remix/routes/kr.search.buy-sell._index-def.js";</script>
        "#;
        let urls = bundle_urls(&base, html);
        assert_eq!(urls.len(), 4);
        assert!(urls[0].contains("page-123.js") || urls[0].contains("_remix"));
        assert!(urls.last().unwrap().contains("polyfills-abc.js"));
    }
}

use super::config::{
    build_stealth_browser_config, cookie_param_for, fingerprint_seed, is_cloakbrowser_engine,
};
use crate::accounts;
use std::path::PathBuf;

#[test]
fn stealth_browser_config_activates_cloakbrowser_flags_without_double_dashes() {
    let engine = crate::environment::BrowserEngine {
        id: "browser-cloak".into(),
        name: "CloakBrowser".into(),
        protocol: "chromium-cdp".into(),
        discovered_from: "cloakbrowser_stealth".into(),
        executable: PathBuf::from("/opt/cloakbrowser/chromium-146.0.7680.177.5/chrome"),
    };
    assert!(is_cloakbrowser_engine(&engine));
    let seed = fingerprint_seed("https://discord.com");
    assert!((10_000..=99_999).contains(&seed));
    assert_eq!(seed, fingerprint_seed("https://discord.com"));

    let cfg = build_stealth_browser_config(&engine, None, "https://discord.com", false)
        .expect("config should build");
    let debug_str = format!("{cfg:?}");
    assert!(debug_str.contains("disable_default_args: true"));
    assert!(debug_str.contains("viewport: None"));
    assert!(debug_str.contains("sandbox: false"));
    assert!(debug_str.contains("New"));
    assert!(debug_str.contains("fingerprint"));
    assert!(debug_str.contains("fingerprint-platform"));
    assert!(!debug_str.contains("\"--"));
}

#[test]
fn cookie_param_for_preserves_host_only_and_subdomain_scoping() {
    let host_only = accounts::Cookie {
        name: "__Host-user_session".into(),
        value: "tok1".into(),
        domain: "github.com".into(),
        path: "/".into(),
        secure: true,
        http_only: true,
        host_only: true,
        expires: None,
        same_site: "lax".into(),
        store_id: String::new(),
        partition_key: None,
    };
    let param = cookie_param_for(&host_only, "https://github.com");
    assert_eq!(param.url.as_deref(), Some("https://github.com/"));
    assert!(param.domain.is_none());

    let domain_scoped = accounts::Cookie {
        name: "ad962bd2sb".into(),
        value: "oauth2_tok".into(),
        domain: ".upwork.com".into(),
        path: "/nx/find-work/".into(),
        secure: true,
        http_only: true,
        host_only: false,
        expires: None,
        same_site: "lax".into(),
        store_id: String::new(),
        partition_key: None,
    };
    let param2 = cookie_param_for(&domain_scoped, "https://www.upwork.com");
    assert_eq!(
        param2.url.as_deref(),
        Some("https://upwork.com/nx/find-work/")
    );
    assert_eq!(param2.domain.as_deref(), Some(".upwork.com"));
    assert_eq!(param2.path.as_deref(), Some("/nx/find-work/"));
}

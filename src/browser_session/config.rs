use crate::{accounts, apperr::AppError, util};
use chromiumoxide::{browser::BrowserConfig, cdp::browser_protocol::network};

pub const STEALTH_INIT_SCRIPT: &str = r#"try {
  delete Object.getPrototypeOf(navigator).webdriver;
  Object.defineProperty(navigator, 'webdriver', { get: () => false, configurable: true });
} catch (_) {}
try {
  if (!window.chrome) {
    Object.defineProperty(window, 'chrome', {
      value: { runtime: {}, app: { isInstalled: false }, csi: () => ({}), loadTimes: () => ({}) },
      configurable: true
    });
  }
} catch (_) {}
try {
  if (!navigator.languages || navigator.languages.length === 0) {
    Object.defineProperty(navigator, 'languages', { get: () => ['en-US', 'en'], configurable: true });
  }
} catch (_) {}
try {
  if (navigator.plugins && navigator.plugins.length === 0) {
    Object.defineProperty(navigator, 'plugins', {
      get: () => [{ name: 'PDF Viewer', filename: 'internal-pdf-viewer', description: 'Portable Document Format' }],
      configurable: true
    });
  }
} catch (_) {}
try {
  if (window.Notification && window.Permissions && Permissions.prototype.query) {
    const origQuery = Permissions.prototype.query;
    Permissions.prototype.query = function(parameters) {
      return parameters && parameters.name === 'notifications'
        ? Promise.resolve({ state: Notification.permission, onchange: null })
        : origQuery.apply(this, arguments);
    };
  }
} catch (_) {}
try {
  if (!window.__hycli_apis && typeof window.fetch === 'function') {
    window.__hycli_apis = [];
    const origFetch = window.fetch;
    window.fetch = async function(...args) {
      const req = args[0];
      const init = args[1] || {};
      const url = typeof req === 'string' ? req : (req && req.url ? req.url : '');
      const method = String((init && init.method) || (req && req.method) || 'GET').toUpperCase();
      const body = typeof init.body === 'string' ? init.body.slice(0, 4096) : '';
      const resp = await origFetch.apply(this, args);
      try {
        if (window.__hycli_apis.length < 40 && resp && resp.ok) {
          const ct = (resp.headers && resp.headers.get && resp.headers.get('content-type')) || '';
          if (ct.includes('json')) {
            const clone = resp.clone();
            clone.json().then((data) => {
              if (window.__hycli_apis.length < 40) {
                window.__hycli_apis.push({ url, method, status: resp.status, body, data });
              }
            }).catch(() => {});
          }
        }
      } catch (_) {}
      return resp;
    };
  }
} catch (_) {}"#;

pub fn is_cloakbrowser_engine(engine: &crate::environment::BrowserEngine) -> bool {
    engine.name.eq_ignore_ascii_case("CloakBrowser")
        || engine.discovered_from == "cloakbrowser_stealth"
        || engine
            .executable
            .to_string_lossy()
            .to_ascii_lowercase()
            .contains("cloakbrowser")
}

pub fn fingerprint_seed(seed_input: &str) -> u32 {
    let digest = util::hash(seed_input.as_bytes());
    let prefix = u32::from_str_radix(&digest[..8], 16).unwrap_or(42_000);
    10_000 + (prefix % 90_000)
}

pub fn can_run_headed() -> bool {
    if std::env::var_os("HYCLI_DISABLE_HEADED").is_some() {
        return false;
    }
    if std::env::var_os("HYCLI_HEADED").is_some() {
        return true;
    }
    #[cfg(any(target_os = "macos", windows))]
    {
        true
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some()
    }
}

pub fn build_stealth_browser_config(
    engine: &crate::environment::BrowserEngine,
    profile_dir: Option<&std::path::Path>,
    seed_input: &str,
    headed: bool,
) -> Result<BrowserConfig, String> {
    let mut builder = BrowserConfig::builder()
        .chrome_executable(&engine.executable)
        .disable_default_args()
        .no_sandbox()
        .viewport(None)
        .env("CLOAKBROWSER_SUPPRESS_FONT_WARNING", "1");
    if let Some(dir) = profile_dir {
        builder = builder.user_data_dir(dir);
    }
    if headed {
        builder = builder
            .with_head()
            .arg("ignore-gpu-blocklist")
            .arg(("window-position", "-32000,-32000"))
            .window_size(1920, 1080);
    } else {
        builder = builder.new_headless_mode().window_size(1920, 947);
        #[cfg(windows)]
        {
            builder = builder.arg("ignore-gpu-blocklist").arg("disable-gpu");
        }
    }
    // Pass clean Chromium argument keys without leading `--` because chromiumoxide's
    // ArgsBuilder formats keys with `format!("--{}", key)`.
    builder = builder
        .arg("no-first-run")
        .arg("no-default-browser-check")
        .arg("disable-background-networking")
        .arg("disable-background-timer-throttling")
        .arg("disable-backgrounding-occluded-windows")
        .arg("disable-breakpad")
        .arg("disable-client-side-phishing-detection")
        .arg("disable-component-extensions-with-background-pages")
        .arg("disable-default-apps")
        .arg("disable-dev-shm-usage")
        .arg("disable-hang-monitor")
        .arg("disable-ipc-flooding-protection")
        .arg("disable-popup-blocking")
        .arg("disable-prompt-on-repost")
        .arg("disable-renderer-backgrounding")
        .arg("disable-sync")
        .arg(("password-store", "basic"))
        .arg("use-mock-keychain")
        .arg((
            "disable-features",
            "OptimizationHints,MediaRouter,DialMediaRouteProvider,Translate",
        ))
        .arg(("disable-blink-features", "AutomationControlled"));

    if is_cloakbrowser_engine(engine) {
        let seed = fingerprint_seed(seed_input).to_string();
        let platform = if cfg!(target_os = "macos") {
            "macos"
        } else {
            "windows"
        };
        builder = builder
            .arg(("fingerprint", seed.as_str()))
            .arg(("fingerprint-platform", platform));
    }
    builder.build()
}

pub(super) fn cookie_param_for(
    cookie: &accounts::Cookie,
    fallback_origin: &str,
) -> network::CookieParam {
    let mut value = network::CookieParam::new(&cookie.name, &cookie.value);
    let scheme = if fallback_origin.starts_with("http://") && !cookie.secure {
        "http"
    } else {
        "https"
    };
    let clean_domain = cookie.domain.trim_start_matches('.');
    let path = if cookie.path.starts_with('/') {
        cookie.path.as_str()
    } else {
        "/"
    };
    if !clean_domain.is_empty() {
        value.url = Some(format!("{scheme}://{clean_domain}{path}"));
    } else {
        value.url = Some(format!("{fallback_origin}/"));
    }
    if !cookie.host_only && !clean_domain.is_empty() {
        value.domain = Some(format!(".{clean_domain}"));
    }
    value.path = Some(path.to_string());
    value.secure = Some(cookie.secure);
    value.http_only = Some(cookie.http_only);
    value.same_site = match cookie.same_site.as_str() {
        "strict" => Some(network::CookieSameSite::Strict),
        "lax" => Some(network::CookieSameSite::Lax),
        "no_restriction" => Some(network::CookieSameSite::None),
        _ => None,
    };
    value
}

pub(super) fn unavailable<T>(_: T) -> AppError {
    AppError::api("browser_unavailable", 422)
}

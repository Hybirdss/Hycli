//! Restore one site's session in a private browser and observe its own read requests.
mod capture;
mod config;
mod in_page;
mod probe;
mod profile_lock;
#[cfg(test)]
mod tests;

pub use capture::{Capture, capture, capture_with_engine, navigation_safe};
pub use config::{
    STEALTH_INIT_SCRIPT, build_stealth_browser_config, can_run_headed, fingerprint_seed,
    is_cloakbrowser_engine,
};
pub use in_page::{InPageFetchRequest, InPageFetchResponse, fetch_in_page};
pub use probe::{Observation, stealth_probe};

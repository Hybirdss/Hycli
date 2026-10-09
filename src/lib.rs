pub mod accounts;
pub mod agent_tools;
pub mod ai;
pub mod apperr;
pub mod browser;
pub mod browser_discovery;
pub mod browser_session;
pub mod browser_storage;
pub mod capture;
pub mod cli_actions;
pub mod defend;
pub mod discovery;
pub mod environment;
pub mod har;
pub mod kb;
pub mod net;
pub mod output;
pub mod platform;
pub mod policy;
pub mod preparation;
pub mod progress;
pub mod runtime;
pub mod session_recipe;
pub mod site_images;
pub mod spec;
pub mod state;
pub mod store;
pub mod util;

pub mod companion;
pub mod dashboard;
pub mod desktop;

pub mod guarded_mcp;
pub mod html;

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod spec_test;

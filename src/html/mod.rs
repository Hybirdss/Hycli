//! Bounded structural HTML evidence and declarative extraction, without script execution.
mod dom;
mod extract;
mod forms;
mod ssr;

pub use dom::{fields, structure};
pub use extract::{Extraction, Field, OutputField, infer_html_extraction};
pub use forms::{forms, nav_list_routes};
pub use ssr::{SSR_GLOBAL_KEYS, extract_ssr_json, extract_ssr_map, resolve_pointer, ssr_payloads};

#[cfg(test)]
mod tests;

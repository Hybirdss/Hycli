//! Declarative SiteSpec format (v1 & v2) for turning websites into typed CLIs.
mod model;
mod pagination;
pub(crate) use pagination::validate_pagination;
mod request;
mod response;
mod types;
mod validate;

pub use request::{FormEncoder, MultipartPart, interpolate_template_str, render_json_template};
pub use types::*;
pub use validate::parse;

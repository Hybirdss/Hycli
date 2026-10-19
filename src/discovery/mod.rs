//! Published documentation discovery. Links and redirects are evidence, never executable instructions.
mod bundles;
mod routes;
mod schema;

pub use bundles::{bundle_api_bases, bundle_routes, bundle_urls};
pub use routes::{
    api_prefixes, documentation_url, documented_routes, header_links, priority, published_links,
    ranked, route_matches,
};
pub use schema::{api_bases, api_schema};

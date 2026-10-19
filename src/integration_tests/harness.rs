pub(super) use crate::{
    policy::{self, Effect},
    runtime::Runtime,
    spec, util,
};
pub(super) use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
pub(super) use serde_json::{Value, json};
pub(super) use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

pub(super) fn root() -> PathBuf {
    std::env::var_os("HYCLI_TEST_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".cache/tests"))
        .join(util::id())
}

pub(super) fn runtime() -> Arc<Runtime> {
    Runtime::open(&root(), None, None, false, true, false).unwrap()
}

pub(super) async fn host(router: Router) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (address, task)
}

pub(super) fn allow_changes(core: &Runtime, site: &str) {
    core.state
        .update(|d| {
            d.sites.entry(site.into()).or_default().writes = true;
            Ok(())
        })
        .unwrap();
}

pub(super) fn spec_for(origin: &str) -> spec::Spec {
    spec::parse(json!({"spec_version":1,"site":{"name":"fixture","title":"Fixture website","base_url":origin},"operations":[{"name":"read-items","method":"GET","path":"/items","effect":"read","evidence":"Synthetic fixture documentation"},{"name":"create-item","method":"POST","path":"/items","effect":"write","evidence":"Synthetic fixture documentation","body":{"json":{"title":{"type":"string","required":true}}}}]}).to_string().as_bytes()).unwrap()
}

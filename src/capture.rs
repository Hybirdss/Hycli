//! 관찰 원료를 엔드포인트 클러스터로. "캡처된 것은 반드시 분류되고, 미검증 표면은 갭이 된다".
use serde::Serialize;

use crate::apperr;
use crate::har;
use crate::spec;

#[derive(Debug, Clone, Serialize)]
pub struct Cluster {
    pub method: String,
    pub template: String,
    pub count: i64,
    pub last: u16,
    pub is_json: bool,
}

pub fn from_har(f: &har::File) -> Vec<Cluster> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Cluster> = Default::default();
    for e in &f.log.entries {
        let Ok(url) = url::Url::parse(&e.request.url) else {
            continue;
        };
        let path = url.path().to_owned();
        if static_asset(&path) {
            continue;
        }
        let tpl = templify(&path);
        let key = format!("{} {}", e.request.method, tpl);
        let c = groups.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Cluster {
                method: e.request.method.clone(),
                template: tpl.clone(),
                count: 0,
                last: 0,
                is_json: false,
            }
        });
        c.count += 1;
        c.last = e.response.status;
        if e.response.content.mime_type.contains("json") {
            c.is_json = true;
        }
    }
    let mut out: Vec<Cluster> = order.iter().filter_map(|k| groups.remove(k)).collect();
    out.sort_by(|a, b| {
        b.is_json
            .cmp(&a.is_json)
            .then(b.count.cmp(&a.count))
            .then(a.template.cmp(&b.template))
    });
    out
}

fn templify(path: &str) -> String {
    let mut idx = 0;
    let segs: Vec<String> = path
        .split('/')
        .map(|seg| {
            if seg.is_empty() {
                return seg.to_string();
            }
            if is_dynamic(seg) {
                let s = format!("{{id{idx}}}");
                idx += 1;
                s
            } else {
                seg.to_string()
            }
        })
        .collect();
    segs.join("/")
}

fn is_dynamic(seg: &str) -> bool {
    let digits = seg.bytes().all(|b| b.is_ascii_digit()) && !seg.is_empty();
    let uuid = seg.len() == 36 && seg.matches('-').count() == 4;
    let hex_long = seg.len() >= 16 && seg.bytes().all(|b| b.is_ascii_hexdigit());
    digits || uuid || hex_long
}

fn static_asset(path: &str) -> bool {
    [
        ".js", ".css", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".woff", ".woff2", ".ico", ".map",
    ]
    .iter()
    .any(|ext| path.to_lowercase().ends_with(ext))
}

pub fn slugify(c: &Cluster) -> String {
    let p: String = c
        .template
        .chars()
        .map(|ch| match ch {
            '{' | '}' | '/' => '-',
            other => other,
        })
        .collect();
    let p = p.trim_matches('-').to_lowercase();
    let p: String = p
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .collect();
    let p = if p.is_empty() { "root".to_string() } else { p };
    format!("{}-{}", c.method.to_lowercase(), p)
}

/// 클러스터 → 검증 통과하는 초안 SiteSpec. 저자(AI)가 할 일: 네이밍·auth·모델.
pub fn draft_spec(
    site_name: &str,
    title: &str,
    base_url: &str,
    clusters: &[Cluster],
    limit: usize,
) -> AppResultSpec {
    let mut sp = spec::Spec {
        spec_version: spec::SPEC_VERSION,
        site: spec::SiteInfo {
            source_url: String::new(),
            name: site_name.into(),
            title: title.into(),
            base_url: base_url.into(),
        },
        defaults: Default::default(),
        auth: vec![spec::AuthStrategy {
            name: "public".into(),
            kind: "none".into(),
            header: String::new(),
            prefix: String::new(),
            value_from: String::new(),
            origin: String::new(),
        }],
        operations: Vec::new(),
        models: Default::default(),
        health: None,
    };
    sp.defaults.timeout = "15s".into();
    sp.defaults.tls_profile = "go".into();

    let limit = if limit == 0 || limit > 100 { 20 } else { limit };
    let mut used = std::collections::HashSet::new();
    for c in clusters.iter().take(limit) {
        let mut name = slugify(c);
        while used.contains(&name) {
            name.push_str("-x");
        }
        used.insert(name.clone());
        let mut op = spec::Operation {
            transport: String::new(),
            name,
            desc: format!("capture draft ({}회 관찰, 마지막 {})", c.count, c.last),
            method: c.method.clone(),
            path: c.template.clone(),
            base_url: String::new(),
            auth: "public".into(),
            params: Default::default(),
            query: Default::default(),
            headers: Default::default(),
            body: None,
            model: String::new(),
            effect: crate::policy::Effect::Unknown,
            evidence: "Observed browser request; effect has not been established.".into(),
            response: None,
        };
        for seg in c.template.split('/') {
            if seg.starts_with('{') {
                let n = seg.trim_matches(|ch| ch == '{' || ch == '}');
                let t = "string";
                op.params.insert(
                    n.to_string(),
                    spec::Param {
                        kind: t.into(),
                        required: true,
                        default: None,
                    },
                );
            }
        }
        sp.operations.push(op);
    }
    let issues = sp.validate();
    if !issues.is_empty() {
        return Err(apperr::spec(
            format!("draft 검증 실패: {issues:?}"),
            "draft.yaml을 수동 편집",
        ));
    }
    Ok(sp)
}

pub type AppResultSpec = apperr::AppResult<spec::Spec>;

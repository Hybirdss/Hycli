use super::dom::stable_name;
use scraper::{Html, Selector};
use serde_json::{Value, json};

pub const SSR_GLOBAL_KEYS: &[&str] = &[
    "__NEXT_DATA__",
    "__remixContext",
    "__NEXT_F__",
    "__APOLLO_STATE__",
    "__NUXT_DATA__",
    "__INITIAL_STATE__",
    "__PRELOADED_STATE__",
    "__STATE__",
    "INIT_DATA",
];

fn collect_next_f_props(val: &Value, out: &mut serde_json::Map<String, Value>, depth: usize) {
    if depth > 24 {
        return;
    }
    match val {
        Value::Object(map) => {
            for (k, v) in map {
                if matches!(
                    k.as_str(),
                    "data"
                        | "posts"
                        | "items"
                        | "articles"
                        | "results"
                        | "initialData"
                        | "nodes"
                        | "edges"
                        | "list"
                        | "feed"
                        | "entries"
                        | "trending"
                        | "searchParams"
                        | "params"
                        | "buildId"
                ) {
                    let non_empty = match v {
                        Value::Null => false,
                        Value::Array(a) => !a.is_empty(),
                        Value::Object(o) => !o.is_empty(),
                        Value::String(s) => !s.is_empty(),
                        _ => true,
                    };
                    if non_empty {
                        out.entry(k.clone()).or_insert_with(|| v.clone());
                    }
                } else if !matches!(
                    k.as_str(),
                    "children" | "fallback" | "parallelRoutes" | "styles" | "tree"
                ) && v
                    .as_array()
                    .is_some_and(|arr| !arr.is_empty() && arr.first().is_some_and(Value::is_object))
                {
                    out.entry(k.clone()).or_insert_with(|| v.clone());
                }
                collect_next_f_props(v, out, depth + 1);
            }
        }
        Value::Array(arr) => {
            for item in arr {
                collect_next_f_props(item, out, depth + 1);
            }
        }
        _ => {}
    }
}

/// Resolve a JSON Pointer (RFC 6901), with fallback matching for object keys that contain unescaped `/`.
pub fn resolve_pointer<'a>(value: &'a Value, pointer: &str) -> Option<&'a Value> {
    if pointer.is_empty() {
        return Some(value);
    }
    if let Some(found) = value.pointer(pointer) {
        return Some(found);
    }
    let rest = pointer.strip_prefix('/')?;
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                if rest == k {
                    return Some(v);
                }
                if let Some(after) = rest
                    .strip_prefix(k.as_str())
                    .and_then(|s| s.strip_prefix('/'))
                {
                    if let Some(found) = resolve_pointer(v, &format!("/{after}")) {
                        return Some(found);
                    }
                }
            }
            None
        }
        Value::Array(arr) => {
            let (idx_str, after) = rest.split_once('/').unwrap_or((rest, ""));
            let idx: usize = idx_str.parse().ok()?;
            let elem = arr.get(idx)?;
            if after.is_empty() {
                Some(elem)
            } else {
                resolve_pointer(elem, &format!("/{after}"))
            }
        }
        _ => None,
    }
}

/// Parse embedded SSR JSON blocks (`__NEXT_DATA__`, `__NEXT_F__`, `__remixContext`, `__NUXT_DATA__`,
/// `application/ld+json`, `window.__INITIAL_STATE__`, `window.__STATE__`, `window.INIT_DATA`) from HTML without executing scripts.
pub fn extract_ssr_map(html: &str) -> std::collections::BTreeMap<String, Value> {
    let document = Html::parse_document(html);
    let mut payloads = std::collections::BTreeMap::new();
    let mut next_f_stream = String::new();
    let window_assign_re = regex::Regex::new(r"window\.([A-Za-z0-9_]+)\s*=\s*").expect("constant");

    if let Ok(sel) = Selector::parse("script") {
        for script in document.select(&sel).take(250) {
            let id = script.value().id().unwrap_or("");
            let script_key = if !id.is_empty() {
                id
            } else {
                script
                    .value()
                    .attr("data-target")
                    .or_else(|| script.value().attr("data-island"))
                    .or_else(|| script.value().attr("data-ssr"))
                    .unwrap_or("")
            };
            let r#type = script.value().attr("type").unwrap_or("");
            let raw: String = script.text().collect();
            let raw = raw.trim();
            if raw.is_empty() || raw.len() > 4 * 1024 * 1024 {
                continue;
            }
            if (!script_key.is_empty()
                && (script_key == "__NEXT_DATA__"
                    || script_key == "__NUXT_DATA__"
                    || r#type.contains("json")
                    || script_key.ends_with(".embeddedData")))
                || r#type.eq_ignore_ascii_case("application/ld+json")
            {
                if let Ok(val) = serde_json::from_str::<Value>(raw) {
                    if !script_key.is_empty() {
                        payloads.insert(script_key.to_owned(), val);
                    } else if let Some(existing) = payloads.get_mut("ld+json") {
                        if let (Value::Object(ex_map), Value::Object(new_map)) = (existing, &val) {
                            for (k, v) in new_map {
                                ex_map.entry(k.clone()).or_insert_with(|| v.clone());
                            }
                            let blocks = ex_map
                                .entry("blocks".to_owned())
                                .or_insert_with(|| Value::Array(vec![]));
                            if let Value::Array(arr) = blocks {
                                arr.push(val);
                            }
                        }
                    } else {
                        payloads.insert("ld+json".to_owned(), val);
                    }
                }
                continue;
            }
            for cap in window_assign_re.captures_iter(raw).take(20) {
                let ident = &cap[1];
                if ident != "INIT_DATA" && !ident.starts_with("__") {
                    continue;
                }
                let Some(m) = cap.get(0) else {
                    continue;
                };
                let rhs = raw[m.end()..].trim_start();
                if !rhs.starts_with('{') && !rhs.starts_with('[') {
                    continue;
                }
                let mut stream = serde_json::Deserializer::from_str(rhs).into_iter::<Value>();
                if let Some(Ok(mut val)) = stream.next() {
                    if ident == "__remixContext" {
                        if let Some(Value::Object(loader_map)) =
                            val.pointer("/state/loaderData").cloned()
                        {
                            let mut route_data = serde_json::Map::new();
                            for (rk, rv) in loader_map {
                                if rk != "root" {
                                    if let Value::Object(rmap) = rv {
                                        for (fk, fv) in rmap {
                                            route_data.entry(fk).or_insert(fv);
                                        }
                                    }
                                }
                            }
                            if route_data
                                .get("buySellArticles")
                                .is_none_or(|v| v.as_array().is_some_and(|a| a.is_empty()))
                            {
                                if let Some(results) = route_data
                                    .get("sections")
                                    .and_then(Value::as_array)
                                    .and_then(|secs| {
                                        secs.iter().find(|s| {
                                            s.get("type").and_then(Value::as_str)
                                                == Some("buy-sell")
                                        })
                                    })
                                    .and_then(|s| s.get("results"))
                                    .cloned()
                                {
                                    route_data.insert("buySellArticles".into(), results);
                                }
                            }
                            if !route_data.is_empty() {
                                if let Value::Object(top) = &mut val {
                                    top.insert("routeData".into(), Value::Object(route_data));
                                }
                            }
                        }
                    } else if ident == "__APOLLO_STATE__" {
                        if let Value::Object(top) = &mut val {
                            let mut items = Vec::new();
                            let mut remove_keys = Vec::new();
                            for (k, v) in top.iter() {
                                if k.contains(':') && !k.starts_with('$') {
                                    if let Value::Object(obj) = v {
                                        if obj.contains_key("title") || obj.contains_key("url_slug")
                                        {
                                            items.push(v.clone());
                                        }
                                        remove_keys.push(k.clone());
                                    }
                                }
                            }
                            for k in remove_keys {
                                top.remove(&k);
                            }
                            if !items.is_empty() {
                                top.insert("items".into(), Value::Array(items));
                            }
                        }
                    }
                    payloads.insert(ident.to_owned(), val);
                }
            }
            let mut search_from = 0;
            while let Some(pos) = raw[search_from..].find("__next_f.push(") {
                let start = search_from + pos + "__next_f.push(".len();
                let rhs = raw[start..].trim_start();
                if rhs.starts_with('[') {
                    let mut stream = serde_json::Deserializer::from_str(rhs).into_iter::<Value>();
                    if let Some(Ok(Value::Array(arr))) = stream.next() {
                        if let Some(Value::String(chunk)) = arr.get(1) {
                            next_f_stream.push_str(chunk);
                        }
                    }
                }
                search_from = start;
            }
        }
    }
    if !next_f_stream.is_empty() {
        let mut next_f_obj = serde_json::Map::new();
        for line in next_f_stream.lines().take(500) {
            let Some((_, rhs)) = line.split_once(':') else {
                continue;
            };
            let rhs = rhs.trim();
            if rhs.starts_with('[') || rhs.starts_with('{') {
                if let Ok(parsed) = serde_json::from_str::<Value>(rhs) {
                    collect_next_f_props(&parsed, &mut next_f_obj, 0);
                }
            }
        }
        if !next_f_obj.is_empty() {
            payloads.insert("__NEXT_F__".to_owned(), Value::Object(next_f_obj));
        }
    }
    payloads
}

/// Extract a specific SSR JSON payload (or `"auto"` for the first available) and optional JSON pointer.
pub fn extract_ssr_json(html: &str, source: &str, pointer: &str) -> Option<Value> {
    let map = extract_ssr_map(html);
    if map.is_empty() {
        return None;
    }
    let root = if source.is_empty() || source == "auto" {
        SSR_GLOBAL_KEYS
            .iter()
            .find_map(|k| map.get(*k))
            .or_else(|| map.values().next())?
    } else {
        map.get(source).or_else(|| {
            if source == "ld+json" {
                None
            } else {
                SSR_GLOBAL_KEYS[..6].iter().find_map(|k| map.get(*k))
            }
        })?
    };
    resolve_pointer(root, pointer).cloned()
}

/// Summarize SSR JSON payload shapes and CSRF/token selector metadata without exposing secret values.
pub fn ssr_payloads(html: &str) -> Value {
    let map = extract_ssr_map(html);
    let mut shapes = serde_json::Map::new();
    for (k, v) in map {
        shapes.insert(k, crate::session_recipe::shape(&v, 0));
    }
    let document = Html::parse_document(html);
    let mut csrf_selectors = Vec::new();
    if let Ok(sel) = Selector::parse("meta[name], input[type=\"hidden\"][name]") {
        for el in document.select(&sel).take(40) {
            let tag = el.value().name();
            if let Some(name) = el.value().attr("name").filter(|n| stable_name(n)) {
                let lower = name.to_ascii_lowercase();
                if ["csrf", "xsrf", "authenticity", "token", "nonce"]
                    .iter()
                    .any(|p| lower.contains(p))
                {
                    csrf_selectors.push(format!("{tag}[name=\"{name}\"]"));
                }
            }
        }
    }
    json!({
        "payloads": shapes,
        "csrf_selectors": csrf_selectors,
    })
}

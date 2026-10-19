use serde_json::{Map, Value};

use crate::apperr::{self, AppResult};

use super::types::Spec;

impl Spec {
    /// Project a raw JSON response into a declared model shape with type coercion.
    pub fn apply_model(&self, name: &str, raw: &[u8]) -> AppResult<Value> {
        let model = match self.models.get(name) {
            Some(m) => m,
            None => return Ok(serde_json::from_slice(raw).unwrap_or(Value::Null)),
        };
        let doc: Value = serde_json::from_slice(raw).map_err(|e| {
            apperr::spec(
                format!("Failed to parse JSON while applying model ({e})"),
                "",
            )
        })?;
        let mut out = Map::new();
        for (field, f) in model {
            let path = if f.path.is_empty() {
                field.clone()
            } else {
                f.path.clone()
            };
            if let Some(v) = walk(&doc, path.split('.')) {
                out.insert(field.clone(), convert_type(v, &f.kind));
            }
        }
        Ok(Value::Object(out))
    }
}

fn walk<'a>(doc: &'a Value, path: impl Iterator<Item = &'a str>) -> Option<&'a Value> {
    let mut cur = doc;
    for seg in path {
        cur = cur.get(seg)?;
    }
    Some(cur)
}

fn convert_type(v: &Value, t: &str) -> Value {
    match t {
        "string" => Value::String(match v {
            Value::String(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
            other => other.to_string(),
        }),
        "int" => v.as_i64().map(Value::from).unwrap_or(Value::Null),
        "float" => v.as_f64().map(Value::from).unwrap_or(Value::Null),
        _ => v.clone(),
    }
}

//! Bounded JSON decoding for untrusted request evidence. Serde's normal Value
//! decoder silently keeps the last duplicate object key; evidence must not.
use std::fmt;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

use crate::apperr::{AppError, AppResult};

const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_DEPTH: usize = 64;

pub(super) fn parse(bytes: &[u8]) -> AppResult<Value> {
    let invalid = || AppError::api("data_invalid", 400);
    if bytes.len() > MAX_BYTES {
        return Err(invalid());
    }
    let mut decoder = serde_json::Deserializer::from_slice(bytes);
    let value = Seed { depth: 0 }
        .deserialize(&mut decoder)
        .map_err(|_| invalid())?;
    decoder.end().map_err(|_| invalid())?;
    Ok(value)
}

struct Seed {
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for Seed {
    type Value = Value;

    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<Value, D::Error> {
        if self.depth > MAX_DEPTH {
            return Err(de::Error::custom("JSON nesting limit"));
        }
        decoder.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Seed {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("bounded JSON without duplicate object keys")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| de::Error::custom("invalid JSON number"))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut input: A) -> Result<Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = input.next_element_seed(Seed {
            depth: self.depth + 1,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut input: A) -> Result<Value, A::Error> {
        let mut values = Map::new();
        while let Some(key) = input.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(de::Error::custom("duplicate JSON key"));
            }
            let value = input.next_value_seed(Seed {
                depth: self.depth + 1,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_normal_json_values() {
        let source = br#"{"a":[null,true,false,1,-2,1.5,"hello"],"b":{}}"#;
        assert_eq!(
            parse(source).unwrap(),
            serde_json::from_slice::<Value>(source).unwrap()
        );
    }

    #[test]
    fn rejects_duplicate_keys_at_every_depth_and_after_unescaping() {
        for source in [
            r#"{"id":"safe","id":"other"}"#,
            r#"{"body":{"query":"safe","query":"other"}}"#,
            r#"[{"id":1,"\u0069d":2}]"#,
        ] {
            let error = parse(source.as_bytes()).unwrap_err();
            assert_eq!(error.public_code, "data_invalid");
            assert_eq!(error.msg, "data_invalid");
        }
    }

    #[test]
    fn bounds_size_depth_and_accepts_no_trailing_data() {
        assert!(parse(&vec![b' '; MAX_BYTES + 1]).is_err());
        let deep = format!(
            "{}0{}",
            "[".repeat(MAX_DEPTH + 1),
            "]".repeat(MAX_DEPTH + 1)
        );
        assert!(parse(deep.as_bytes()).is_err());
        assert!(parse(b"{} {}").is_err());
        assert!(parse(b"\xff").is_err());
        assert!(
            parse(b"{\"secret-token\":")
                .unwrap_err()
                .to_string()
                .find("secret-token")
                .is_none()
        );
    }
}

// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![allow(clippy::missing_errors_doc)] // parse/stringify errors are plain String messages.

//! Bounded JSON and the `BNJson` document table. This lives in `bn_rt`, not in
//! the interpreter provider, so `bni` and `bnc` share one table and one set of
//! bounds — a handle means the same thing on both paths (W3). Relocated from
//! `bn_lib_json` in bucket 0.6.1c.

use serde::de::{DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};

pub const MAX_DEPTH: usize = 64;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;

pub fn parse(input: &str) -> Result<serde_json::Value, String> {
    if input.len() > MAX_BYTES {
        return Err("JSON input exceeds 8 MiB".into());
    }
    let mut deserializer = serde_json::Deserializer::from_str(input);
    let value = JsonSeed { depth: 0 }
        .deserialize(&mut deserializer)
        .map_err(|error| error.to_string())?;
    deserializer.end().map_err(|error| {
        if error.is_data() {
            "trailing JSON input".into()
        } else {
            error.to_string()
        }
    })?;
    Ok(value)
}

pub fn stringify(value: &serde_json::Value) -> Result<String, String> {
    ensure_depth(value, 0)?;
    let output = serde_json::to_string(value).map_err(|error| error.to_string())?;
    if output.len() > MAX_BYTES {
        return Err("JSON output exceeds 8 MiB".into());
    }
    Ok(output)
}

fn ensure_depth(value: &serde_json::Value, depth: usize) -> Result<(), String> {
    match value {
        serde_json::Value::Array(values) => {
            if depth >= MAX_DEPTH {
                return Err("JSON nesting exceeds 64 levels".into());
            }
            values
                .iter()
                .try_for_each(|value| ensure_depth(value, depth + 1))
        }
        serde_json::Value::Object(values) => {
            if depth >= MAX_DEPTH {
                return Err("JSON nesting exceeds 64 levels".into());
            }
            values
                .values()
                .try_for_each(|value| ensure_depth(value, depth + 1))
        }
        _ => Ok(()),
    }
}

struct JsonSeed {
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for JsonSeed {
    type Value = serde_json::Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(JsonVisitor { depth: self.depth })
    }
}

struct JsonVisitor {
    depth: usize,
}

impl<'de> Visitor<'de> for JsonVisitor {
    type Value = serde_json::Value;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a valid bounded JSON value")
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(serde_json::Value::Null)
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(serde_json::Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(serde_json::Value::Number(value.into()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(serde_json::Value::Number(value.into()))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .ok_or_else(|| E::custom("JSON number is not finite"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(serde_json::Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(serde_json::Value::String(value))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        if self.depth >= MAX_DEPTH {
            return Err(A::Error::custom("JSON nesting exceeds 64 levels"));
        }
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(JsonSeed {
            depth: self.depth + 1,
        })? {
            values.push(value);
        }
        Ok(serde_json::Value::Array(values))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        if self.depth >= MAX_DEPTH {
            return Err(A::Error::custom("JSON nesting exceeds 64 levels"));
        }
        let mut values = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if values.contains_key(&key) {
                return Err(A::Error::custom("duplicate JSON object key"));
            }
            let value = map.next_value_seed(JsonSeed {
                depth: self.depth + 1,
            })?;
            values.insert(key, value);
        }
        Ok(serde_json::Value::Object(values))
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_BYTES, parse, stringify};

    #[test]
    fn round_trips_bounded_json() {
        let value = parse(r#"{"ok":true,"items":[1,null]}"#).unwrap();
        assert_eq!(parse(&stringify(&value).unwrap()), Ok(value));
    }

    #[test]
    fn rejects_duplicate_keys_and_trailing_data() {
        assert!(parse(r#"{"x":1,"x":2}"#).is_err());
        assert!(parse("null false").is_err());
    }

    #[test]
    fn rejects_numbers_outside_json_grammar() {
        assert!(parse("01").is_err());
        assert!(parse("1.").is_err());
        assert!(parse("+1").is_err());
    }

    #[test]
    fn decodes_unicode_surrogate_pairs() {
        assert_eq!(parse(r#""\uD83D\uDE00""#).unwrap(), serde_json::json!("😀"));
        assert!(parse(r#""\uD83D""#).is_err());
    }

    #[test]
    fn enforces_input_byte_limit() {
        let input = format!("\"{}\"", "x".repeat(MAX_BYTES - 2));
        assert!(parse(&input).is_ok());
        assert!(parse(&format!("{input}x")).is_err());
    }

    #[test]
    fn enforces_depth_limit() {
        let depth_64 = format!("{}null{}", "[".repeat(64), "]".repeat(64));
        let depth_65 = format!("{}null{}", "[".repeat(65), "]".repeat(65));
        assert!(parse(&depth_64).is_ok());
        assert!(parse(&depth_65).is_err());
    }
}

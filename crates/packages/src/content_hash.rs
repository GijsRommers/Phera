//! Composer's `content-hash`: MD5 of selected, top-level sorted manifest fields.
//!
//! Editing `description` keeps the hash; editing `require` invalidates the lock.

use anyhow::Result;
use md5::{Digest, Md5};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

pub(crate) fn compute(manifest: &Value) -> Result<String> {
    let mut relevant = BTreeMap::new();
    for key in [
        "name",
        "version",
        "require",
        "require-dev",
        "conflict",
        "replace",
        "provide",
        "minimum-stability",
        "prefer-stable",
        "repositories",
        "extra",
    ] {
        if let Some(value) = manifest.get(key) {
            relevant.insert(key, value.clone());
        }
    }
    let mut config = Map::new();
    for key in ["platform", "policy"] {
        if let Some(value) = manifest["config"].get(key).filter(|value| !value.is_null()) {
            config.insert(key.to_owned(), value.clone());
        }
    }
    if !config.is_empty() {
        relevant.insert("config", Value::Object(config));
    }
    // PHP decodes objects as associative arrays. Empty JSON objects thus encode as [].
    let value = php_arrays(serde_json::to_value(relevant)?);
    let json = serde_json::to_string(&value)?;
    let mut encoded = String::new();
    for character in json.chars() {
        match character {
            '/' => encoded.push_str("\\/"),
            character if !character.is_ascii() => {
                for unit in character.encode_utf16(&mut [0; 2]) {
                    encoded.push_str(&format!("\\u{unit:04x}"));
                }
            }
            _ => encoded.push(character),
        }
    }
    Ok(format!("{:x}", Md5::digest(encoded.as_bytes())))
}

fn php_arrays(value: Value) -> Value {
    match value {
        Value::Object(map) if map.is_empty() => Value::Array(Vec::new()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, php_arrays(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(php_arrays).collect()),
        value => value,
    }
}

#[cfg(test)]
mod tests {
    use super::compute;

    #[test]
    fn composer_2_11_hashes() {
        // Observed from Composer 2.11.2 Locker::getContentHash under PHP 8.5.12.
        for (json, expected) in [
            (r#"{}"#, "d751713988987e9331980363e24189ce"),
            (
                r#"{"require":{"psr/log":"^3"}}"#,
                "80d395326855bcf85a83897344dc6f0e",
            ),
            (
                r#"{"require":{},"extra":{"é":"😀/x"},"config":{"platform":{"php":"8.5"}}}"#,
                "52c810fe21c39601e6269177ca3b3ec5",
            ),
        ] {
            assert_eq!(
                compute(&serde_json::from_str(json).unwrap()).unwrap(),
                expected
            );
        }
    }
}

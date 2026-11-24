//! Persist registry responses with HTTP validators.
//!
//! A warm `phera update` sends `If-None-Match`; offline mode reads only disk.

use crate::{network::Network, store::Store};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;

pub(crate) fn fetch(store: &Store, network: &Network, url: &str, offline: bool) -> Result<Value> {
    let key = format!(
        "{}.json",
        base16ct::lower::encode_string(&Sha256::digest(url.as_bytes()))
    );
    let path = store.directory.join("metadata").join(key);
    let cached: Option<Value> = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    if offline {
        return cached.map(|record| record["body"].clone()).context(
            "Offline metadata cache is missing a repository response; update online first.",
        );
    }
    let record = cached.as_ref().unwrap_or(&Value::Null);
    let response = network.response(url, record["etag"].as_str(), record["modified"].as_str())?;
    if response.status().as_u16() == 304 {
        return cached
            .map(|record| record["body"].clone())
            .context("Repository returned 304 without a cached response; clear metadata cache.");
    }
    let missing = response.status().as_u16() == 404;
    if !missing && !response.status().is_success() {
        bail!(
            "Repository returned HTTP {}; check repository access.",
            response.status()
        );
    }
    let etag = response
        .headers()
        .get("etag")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let modified = response
        .headers()
        .get("last-modified")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body: Value = if missing {
        json!({"packages": {}})
    } else {
        serde_json::from_slice(&crate::network::bounded(response)?)
            .context("Repository returned invalid JSON metadata.")?
    };
    let record = json!({"body": body, "etag": etag, "modified": modified});
    let mut file = tempfile::NamedTempFile::new_in(store.directory.join("metadata"))?;
    file.write_all(&serde_json::to_vec(&record)?)?;
    file.persist(path)?;
    Ok(body)
}

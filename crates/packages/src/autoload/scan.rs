//! Scan PHP declarations with Phera's tokenizer on all cores.
//!
//! `namespace App; enum Status {}` adds `App\Status` to the optimized class map.

use anyhow::{Context, Result};
use rayon::prelude::*;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub(super) struct Source {
    directory: PathBuf,
    pub(super) exclusions: Vec<Regex>,
    psr: Option<(String, bool)>,
}

impl Source {
    pub(super) fn new(
        base: &Path,
        path: &str,
        exclusions: &[String],
        psr: Option<(String, bool)>,
    ) -> Result<Self> {
        let exclusions = patterns(base, exclusions)?;
        let directory = base.join(crate::paths::relative(path)?);
        Ok(Self {
            directory: directory.canonicalize().unwrap_or(directory),
            exclusions,
            psr,
        })
    }
}

pub(super) fn patterns(base: &Path, exclusions: &[String]) -> Result<Vec<Regex>> {
    let base = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
    exclusions
        .iter()
        .map(|pattern| {
            let escaped = regex::escape(pattern.trim_matches('/'))
                .replace("\\*\\*", ".+?")
                .replace("\\*", "[^/]+?");
            Regex::new(&format!(
                "^{}/{}($|/)",
                regex::escape(&base.to_string_lossy()),
                escaped
            ))
        })
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

pub(super) fn classes(
    sources: &[Source],
    vendor: &Path,
    cache: &Path,
) -> Result<BTreeMap<String, PathBuf>> {
    fs::create_dir_all(cache)?;
    let mut files = Vec::new();
    for source in sources {
        for path in glob::glob(&source.directory.to_string_lossy())? {
            let path = path?.canonicalize()?;
            let walker = walkdir::WalkDir::new(&path)
                .sort_by_file_name()
                .into_iter()
                .filter_entry(|entry| entry.path() != vendor);
            for entry in walker {
                let entry = entry?;
                if !entry.file_type().is_file() {
                    continue;
                }
                if !matches!(
                    entry.path().extension().and_then(|ext| ext.to_str()),
                    Some("php" | "inc" | "hh")
                ) {
                    continue;
                }
                if source
                    .exclusions
                    .iter()
                    .any(|pattern| pattern.is_match(&entry.path().to_string_lossy()))
                {
                    continue;
                }
                files.push((entry.into_path(), source));
            }
        }
    }
    let discovered: Vec<Vec<(String, PathBuf)>> = files
        .par_iter()
        .map(|(path, source)| -> Result<_> {
            let text = fs::read(path)
                .with_context(|| format!("Cannot read PHP file {}.", path.display()))?;
            let mut digest = Sha256::new();
            digest.update(b"class-map-v3");
            digest.update(env!("CARGO_PKG_VERSION"));
            digest.update(&text);
            let key = format!("{:x}.json", digest.finalize());
            let cache_file = cache.join(key);
            let cached: Option<Vec<String>> = fs::read(&cache_file)
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok());
            let names = if let Some(names) = cached {
                names
            } else {
                let names = super::declarations::names(&text).with_context(|| {
                    format!("Cannot scan PHP declarations in {}.", path.display())
                })?;
                let mut record = tempfile::NamedTempFile::new_in(cache)?;
                record.write_all(&serde_json::to_vec(&names)?)?;
                record.persist(&cache_file)?;
                names
            };
            Ok(names
                .into_iter()
                .filter(|name| compliant(name, path, source))
                .map(|name| (name, path.clone()))
                .collect())
        })
        .collect::<Result<_>>()?;
    let mut classes = BTreeMap::new();
    for declarations in discovered {
        for (name, path) in declarations {
            classes.entry(name).or_insert(path);
        }
    }
    Ok(classes)
}

fn compliant(name: &str, path: &Path, source: &Source) -> bool {
    let Some((prefix, psr4)) = &source.psr else {
        return true;
    };
    let Some(suffix) = name.strip_prefix(prefix) else {
        return false;
    };
    let relative = if *psr4 {
        suffix.replace('\\', "/")
    } else {
        let (namespace, class) = name.rsplit_once('\\').unwrap_or(("", name));
        let class = class.replace('_', "/");
        if namespace.is_empty() {
            class
        } else {
            format!("{}/{class}", namespace.replace('\\', "/"))
        }
    };
    source.directory.join(format!("{relative}.php")) == path
}

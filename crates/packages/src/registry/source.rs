//! Apply Composer repository priority and include/exclude package filters.
use crate::Package;
use anyhow::{Context, Result};
use serde_json::Value;

pub(super) enum Source {
    Composer { base: String, index: Value },
    Local(Vec<Package>),
}

pub(super) struct Repository {
    pub(super) source: Source,
    pub(super) canonical: bool,
    only: Option<Vec<glob::Pattern>>,
    exclude: Vec<glob::Pattern>,
}

impl Repository {
    pub(super) fn new(source: Source, configuration: &Value) -> Result<Self> {
        let patterns = |key: &str| -> Result<_> {
            configuration
                .get(key)
                .map(|patterns| {
                    let patterns = patterns
                        .as_array()
                        .with_context(|| format!("{key} must be a list."))?;
                    patterns
                        .iter()
                        .map(|pattern| {
                            let text = pattern
                                .as_str()
                                .with_context(|| format!("Invalid {key} pattern."))?;
                            Ok(glob::Pattern::new(text)?)
                        })
                        .collect::<Result<Vec<_>>>()
                })
                .transpose()
        };
        Ok(Self {
            source,
            canonical: configuration["canonical"].as_bool().unwrap_or(true),
            only: patterns("only")?,
            exclude: patterns("exclude")?.unwrap_or_default(),
        })
    }

    pub(super) fn accepts(&self, name: &str) -> bool {
        self.only
            .as_ref()
            .is_none_or(|patterns| patterns.iter().any(|pattern| pattern.matches(name)))
            && !self.exclude.iter().any(|pattern| pattern.matches(name))
    }
}

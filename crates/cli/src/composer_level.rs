//! Infer the language level from Composer's PHP platform requirement.
//!
//! With `"require": {"php": "^8.6"}`, `echo PHP_MINOR_VERSION;` prints 6.

use phera_source::LanguageLevel;
use serde_json::Value;
use std::path::Path;

pub(crate) fn read(path: &Path, source: &str) -> Result<Option<LanguageLevel>, String> {
    let manifest: Value =
        serde_json::from_str(source).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(manifest["require"]["php"].as_str().and_then(infer))
}

fn infer(requirement: &str) -> Option<LanguageLevel> {
    let minimum = requirement
        .split('|')
        .filter(|alternative| !alternative.trim().is_empty())
        .map(lower_bound)
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .flatten()
        .min()?;
    LanguageLevel::ALL.into_iter().find(|level| {
        let (major, minor, _) = level.components();
        (Some(minimum.0), Some(minimum.1)) == (major.try_into().ok(), minor.try_into().ok())
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Version(u64, u64, u64);

impl Version {
    fn next_patch(self) -> Option<Self> {
        Some(Self(self.0, self.1, self.2.checked_add(1)?))
    }

    fn next_minor(self) -> Option<Self> {
        Some(Self(self.0, self.1.checked_add(1)?, 0))
    }

    fn next_major(self) -> Option<Self> {
        Some(Self(self.0.checked_add(1)?, 0, 0))
    }
}

#[derive(Default)]
struct Range {
    minimum: Version,
    upper: Option<Version>,
    excluded: Vec<Version>,
}

impl Range {
    fn limit(&mut self, upper: Version) {
        self.upper = Some(self.upper.map_or(upper, |previous| previous.min(upper)));
    }

    fn apply(&mut self, operator: &str, text: &str) -> Option<()> {
        let (version, parts, wildcard) = version(text)?;
        if wildcard {
            if !matches!(operator, "" | "=" | "==") {
                return None;
            }
            match parts {
                0 => return Some(()),
                1 => self.limit(version.next_major()?),
                2 => self.limit(version.next_minor()?),
                _ => return None,
            }
        } else {
            match operator {
                "<" => self.limit(version),
                "<=" => self.limit(version.next_patch()?),
                "!=" | "<>" => self.excluded.push(version),
                ">" => self.minimum = self.minimum.max(version.next_patch()?),
                ">=" => {}
                "^" => {
                    self.limit(match version {
                        Version(0, 0, _) if parts == 3 => version.next_patch()?,
                        Version(0, _, _) if parts >= 2 => version.next_minor()?,
                        _ => version.next_major()?,
                    });
                }
                "~" => self.limit(if parts >= 3 {
                    version.next_minor()?
                } else {
                    version.next_major()?
                }),
                "" | "=" | "==" => self.limit(version.next_patch()?),
                _ => return None,
            }
        }
        if !matches!(operator, "<" | "<=" | "!=" | "<>") {
            self.minimum = self.minimum.max(version);
        }
        Some(())
    }

    fn first(mut self) -> Option<Option<Version>> {
        while self.excluded.contains(&self.minimum) {
            self.minimum = self.minimum.next_patch()?;
        }
        Some(
            self.upper
                .is_none_or(|upper| self.minimum < upper)
                .then_some(self.minimum),
        )
    }
}

fn lower_bound(alternative: &str) -> Option<Option<Version>> {
    let mut terms = alternative
        .split(|ch: char| ch == ',' || ch.is_whitespace())
        .filter(|term| !term.is_empty())
        .peekable();
    let mut range = Range::default();
    while let Some(term) = terms.next() {
        if terms.next_if_eq(&"-").is_some() {
            let (start, _, false) = version(term)? else {
                return None;
            };
            let (end, parts, false) = version(terms.next()?)? else {
                return None;
            };
            range.minimum = range.minimum.max(start);
            range.limit(match parts {
                1 => end.next_major()?,
                2 => end.next_minor()?,
                _ => end.next_patch()?,
            });
            continue;
        }
        let start = term
            .find(|ch: char| !"<>!=~^".contains(ch))
            .unwrap_or(term.len());
        let (operator, text) = term.split_at(start);
        let text = if text.is_empty() { terms.next()? } else { text };
        range.apply(operator, text)?;
    }
    range.first()
}

fn version(text: &str) -> Option<(Version, usize, bool)> {
    let text = text.trim_start_matches(['v', 'V']);
    let text = text.split_once('@').map_or(text, |(number, _)| number);
    let mut numbers = [0; 3];
    let mut parts = 0;
    let mut wildcard = false;
    for (index, part) in text.split('.').enumerate() {
        if matches!(part, "*" | "x" | "X") {
            wildcard = true;
        } else if !wildcard {
            *numbers.get_mut(index)? = part.parse().ok()?;
            parts += 1;
        } else {
            return None;
        }
    }
    let [major, minor, patch] = numbers;
    Some((Version(major, minor, patch), parts, wildcard))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_the_lower_bound_of_composer_ranges() {
        for requirement in [
            ">=8.6",
            "^8.6",
            "~8.6.0",
            "8.6.*",
            ">= 8.6 < 9",
            "8.6 - 8.9",
            ">=8.6, !=8.6.0",
        ] {
            assert_eq!(
                infer(requirement),
                Some(LanguageLevel::Php86),
                "{requirement}"
            );
        }
        for requirement in ["^8.5 || ^8.6", ">=8.5, <8.7", "8.5.* | ^8.6"] {
            assert_eq!(
                infer(requirement),
                Some(LanguageLevel::Php85),
                "{requirement}"
            );
        }
        for requirement in ["^8.4 || ^8.6", ">=8.7", "dev-main", "*"] {
            assert_eq!(infer(requirement), None, "{requirement}");
        }
    }
}

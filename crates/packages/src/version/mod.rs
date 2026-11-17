//! Composer version normalization and comparison, ported from composer/semver.
//!
//! `Version::parse("v1.2.0RC1")` normalizes to `1.2.0.0-RC1`.

pub(crate) mod constraint;
mod normalize;
mod syntax;

pub use constraint::Constraint;
pub use normalize::normalize;

use anyhow::Result;
use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, Debug)]
pub struct Version {
    normalized: String,
}

impl Version {
    pub fn parse(text: &str) -> Result<Self> {
        Ok(Self {
            normalized: normalize(text)?,
        })
    }

    pub fn normalized(&self) -> &str {
        &self.normalized
    }

    pub fn stability(&self) -> Stability {
        let text = self.normalized.to_ascii_lowercase();
        if text.starts_with("dev-") || text.ends_with("-dev") {
            Stability::Dev
        } else if text.contains("-alpha") {
            Stability::Alpha
        } else if text.contains("-beta") {
            Stability::Beta
        } else if text.contains("-rc") {
            Stability::Rc
        } else {
            Stability::Stable
        }
    }

    pub(crate) fn is_branch(&self) -> bool {
        self.normalized.starts_with("dev-")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stability {
    Dev,
    Alpha,
    Beta,
    Rc,
    Stable,
}

impl Stability {
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "dev" => Some(Self::Dev),
            "alpha" | "a" => Some(Self::Alpha),
            "beta" | "b" => Some(Self::Beta),
            "rc" => Some(Self::Rc),
            "stable" => Some(Self::Stable),
            _ => None,
        }
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Version {}
impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.is_branch() || other.is_branch() {
            let default = |version: &Version| {
                matches!(
                    version.normalized.as_str(),
                    "dev-master" | "dev-trunk" | "dev-default"
                )
            };
            return default(self).cmp(&default(other)).then_with(|| {
                match (self.is_branch(), other.is_branch()) {
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    _ => self.normalized.cmp(&other.normalized),
                }
            });
        }
        let left = parts(&self.normalized);
        let right = parts(&other.normalized);
        for index in 0..left.len().max(right.len()) {
            let left = left.get(index).copied();
            let right = right.get(index).copied();
            let order = match (left, right) {
                (Some(left), Some(right)) => match (left.parse::<u64>(), right.parse::<u64>()) {
                    (Ok(left), Ok(right)) => left.cmp(&right),
                    (Ok(_), Err(_)) => 4.cmp(&rank(right)),
                    (Err(_), Ok(_)) => rank(left).cmp(&4),
                    (Err(_), Err(_)) => rank(left).cmp(&rank(right)),
                },
                (Some(left), None) => {
                    if left.parse::<u64>().is_ok() {
                        Ordering::Greater
                    } else {
                        rank(left).cmp(&4)
                    }
                }
                (None, Some(right)) => {
                    if right.parse::<u64>().is_ok() {
                        Ordering::Less
                    } else {
                        4.cmp(&rank(right))
                    }
                }
                (None, None) => Ordering::Equal,
            };
            if order != Ordering::Equal {
                return order;
            }
        }
        Ordering::Equal
    }
}

fn parts(text: &str) -> Vec<&str> {
    let mut output = Vec::new();
    let mut start = 0;
    let mut digit = None;
    for (index, character) in text.char_indices() {
        if !character.is_ascii_alphanumeric() {
            if start < index {
                output.push(&text[start..index]);
            }
            start = index + character.len_utf8();
            digit = None;
        } else {
            let next_digit = character.is_ascii_digit();
            if digit.is_some_and(|previous| previous != next_digit) {
                output.push(&text[start..index]);
                start = index;
            }
            digit = Some(next_digit);
        }
    }
    if start < text.len() {
        output.push(&text[start..]);
    }
    output
}

fn rank(text: &str) -> u8 {
    match text.to_ascii_lowercase().as_str() {
        "dev" => 0,
        "alpha" | "a" => 1,
        "beta" | "b" => 2,
        "rc" => 3,
        "patch" | "pl" | "p" => 5,
        _ => 0,
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.normalized)
    }
}

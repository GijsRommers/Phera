//! Composer constraints as disjunctions of comparator conjunctions.
//!
//! `^1.2 || ~2.0` accepts 1.9 and 2.7 but rejects 3.0 and unrelated dev branches.

use super::Version;
use anyhow::{Result, bail};
use regex::Regex;
use std::sync::LazyLock;

static SEPARATORS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r",\s*,").expect("valid separator regex"));
static OPERATORS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(>=|<=|!=|<>|==|[=<>~^])\s+").expect("valid operator regex"));

static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?:[^,\s]+\s+as\s+[^,\s]+|[^,\s]+\s+-\s+[^,\s]+|(?:>=|<=|!=|<>|==|[=<>~^])?\s*[^,\s]+)",
    )
    .expect("valid constraint token regex")
});

#[derive(Clone, Copy, Debug)]
pub(crate) enum Operator {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Clone, Debug)]
pub(crate) struct Term {
    pub operator: Operator,
    pub version: Version,
}

#[derive(Clone, Debug)]
pub struct Constraint {
    pub(crate) groups: Vec<Vec<Term>>,
}

impl Constraint {
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim();
        if text.is_empty() || text.contains("|||") {
            bail!("Invalid empty constraint or separator.");
        }
        if text.starts_with(',') || text.ends_with(',') || SEPARATORS.is_match(text) {
            bail!("Invalid comma separator in constraint.");
        }
        let split = text.replace("||", "|");
        let split = OPERATORS.replace_all(&split, "$1");
        let mut groups = Vec::new();
        for group in split.split('|') {
            if group.trim().is_empty() {
                bail!("Empty alternative in constraint.");
            }
            let mut terms = Vec::new();
            for token in TOKEN.find_iter(group.trim()) {
                super::syntax::parse_token(token.as_str().trim(), &mut terms)?;
            }
            groups.push(terms);
        }
        if groups.is_empty() {
            groups.push(Vec::new());
        }
        Ok(Self { groups })
    }

    pub fn matches(&self, version: &Version) -> bool {
        self.groups
            .iter()
            .any(|terms| terms.iter().all(|term| term.matches(version)))
    }

    pub fn intersects(&self, other: &Self) -> bool {
        self.groups.iter().any(|left| {
            other.groups.iter().any(|right| {
                let terms: Vec<_> = left.iter().chain(right).collect();
                if let Some(exact) = terms
                    .iter()
                    .find(|term| matches!(term.operator, Operator::Equal))
                {
                    return terms.iter().all(|term| term.matches(&exact.version));
                }
                let mut lower: Option<(&Version, bool)> = None;
                let mut upper: Option<(&Version, bool)> = None;
                for term in &terms {
                    match term.operator {
                        Operator::Greater | Operator::GreaterEqual => {
                            let inclusive = matches!(term.operator, Operator::GreaterEqual);
                            if lower.is_none_or(|(version, old)| {
                                term.version > *version
                                    || term.version == *version && !inclusive && old
                            }) {
                                lower = Some((&term.version, inclusive));
                            }
                        }
                        Operator::Less | Operator::LessEqual => {
                            let inclusive = matches!(term.operator, Operator::LessEqual);
                            if upper.is_none_or(|(version, old)| {
                                term.version < *version
                                    || term.version == *version && !inclusive && old
                            }) {
                                upper = Some((&term.version, inclusive));
                            }
                        }
                        _ => {}
                    }
                }
                match (lower, upper) {
                    (Some((low, low_inclusive)), Some((high, high_inclusive))) => {
                        low < high
                            || low == high
                                && low_inclusive
                                && high_inclusive
                                && terms.iter().all(|term| term.matches(low))
                    }
                    _ => true,
                }
            })
        })
    }
}

impl Term {
    fn matches(&self, version: &Version) -> bool {
        if self.version.is_branch() || version.is_branch() {
            return match self.operator {
                Operator::Equal => self.version.normalized == version.normalized,
                Operator::NotEqual => self.version.normalized != version.normalized,
                _ => false,
            };
        }
        match self.operator {
            Operator::Equal => *version == self.version,
            Operator::NotEqual => *version != self.version,
            Operator::Less => *version < self.version,
            Operator::LessEqual => *version <= self.version,
            Operator::Greater => *version > self.version,
            Operator::GreaterEqual => *version >= self.version,
        }
    }
}

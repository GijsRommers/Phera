//! Composer constraints as disjunctions of comparator conjunctions.
//!
//! `^1.2 || ~2.0` accepts 1.9 and 2.7 but rejects 3.0 and unrelated dev branches.

use super::Version;
use anyhow::{Result, bail};
use regex::Regex;
use std::sync::LazyLock;

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
        let separators = Regex::new(r",\s*,")?;
        if text.starts_with(',') || text.ends_with(',') || separators.is_match(text) {
            bail!("Invalid comma separator in constraint.");
        }
        let split = text.replace("||", "|");
        let mut groups = Vec::new();
        for group in split.split('|') {
            if group.trim().is_empty() {
                bail!("Empty alternative in constraint.");
            }
            let mut terms = Vec::new();
            for token in TOKEN.find_iter(group.trim()) {
                parse_token(token.as_str().trim(), &mut terms)?;
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

fn parse_token(token: &str, terms: &mut Vec<Term>) -> Result<()> {
    let token = token.split_once(" as ").map_or(token, |(source, _)| source);
    let (token, stability) = token
        .rsplit_once('@')
        .map_or((token, None), |(text, stability)| {
            (if text.is_empty() { "*" } else { text }, Some(stability))
        });
    let token = if token.starts_with("dev-") || token.contains(".x-dev") {
        token.split_once('#').map_or(token, |(version, _)| version)
    } else {
        token
    };
    if matches!(token, "" | "*") {
        return Ok(());
    }
    if token
        .trim_start_matches('v')
        .split('.')
        .all(|part| matches!(part, "x" | "X" | "*"))
    {
        return push(terms, Operator::GreaterEqual, "0.0.0.0-dev");
    }
    if let Some((low, high)) = token.split_once(" - ") {
        push(terms, Operator::GreaterEqual, &lower(low)?)?;
        if [low, high].iter().any(|text| {
            let count = text.matches(['x', 'X', '*']).count();
            count > 0 && (count != 1 || !text.ends_with("-dev"))
        }) {
            bail!("Wildcards cannot be used in a hyphen range.");
        }
        let (numbers, count) = numbers(high);
        if count >= 3 || has_stability(high) {
            return push(terms, Operator::LessEqual, high);
        }
        return push(
            terms,
            Operator::Less,
            &increment(numbers, count.saturating_sub(1)),
        );
    }
    if token.starts_with("~>") {
        bail!("Invalid ~> constraint; use ~ instead.");
    }
    if token.starts_with(['^', '~']) {
        let version = &token[1..];
        let (numbers, count) = numbers(version);
        if count == 0 {
            bail!("Invalid range {token:?}.");
        }
        let position = if token.starts_with('~') {
            (count + usize::from(version.contains(".x-dev") || version.contains(".*-dev")))
                .saturating_sub(2)
        } else if numbers[0] != 0 || count == 1 {
            0
        } else if numbers[1] != 0 || count == 2 {
            1
        } else {
            2
        };
        push(terms, Operator::GreaterEqual, &lower(version)?)?;
        return push(terms, Operator::Less, &increment(numbers, position));
    }
    if token.contains(['*', 'x', 'X']) && !token.starts_with("dev-") && !token.ends_with("dev") {
        let (numbers, count) = numbers(token);
        if count == 0 {
            bail!("Invalid wildcard constraint {token:?}.");
        }
        let low = format!(
            "{}.{}.{}.{}-dev",
            numbers[0], numbers[1], numbers[2], numbers[3]
        );
        if numbers != [0; 4] {
            push(terms, Operator::GreaterEqual, &low)?;
        }
        return push(terms, Operator::Less, &increment(numbers, count - 1));
    }
    let (operator, version) = [
        ("!=", Operator::NotEqual),
        ("<>", Operator::NotEqual),
        (">=", Operator::GreaterEqual),
        ("<=", Operator::LessEqual),
        ("==", Operator::Equal),
        ("=", Operator::Equal),
        (">", Operator::Greater),
        ("<", Operator::Less),
    ]
    .into_iter()
    .find_map(|(prefix, operator)| {
        token
            .strip_prefix(prefix)
            .map(|version| (operator, version.trim()))
    })
    .unwrap_or((Operator::Equal, token));
    let mut normalized = Version::parse(version)
        .or_else(|error| {
            if let Some(branch) = version.strip_suffix("-dev").filter(|branch| {
                branch
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "-./".contains(character))
            }) {
                Version::parse(&format!("dev-{branch}"))
            } else {
                Err(error)
            }
        })?
        .normalized;
    if !matches!(operator, Operator::Equal)
        && !has_stability(version)
        && !normalized.starts_with("dev-")
    {
        if let Some(stability) = stability.filter(|text| *text != "stable") {
            normalized.push('-');
            normalized.push_str(stability);
        } else if matches!(operator, Operator::Less | Operator::GreaterEqual) {
            normalized.push_str("-dev");
        }
    }
    push(terms, operator, &normalized)
}

fn push(terms: &mut Vec<Term>, operator: Operator, text: &str) -> Result<()> {
    terms.push(Term {
        operator,
        version: Version::parse(text)?,
    });
    Ok(())
}

fn lower(text: &str) -> Result<String> {
    let version = Version::parse(text)?;
    Ok(if has_stability(text) {
        version.normalized
    } else {
        format!("{version}-dev")
    })
}

fn has_stability(text: &str) -> bool {
    text.chars()
        .any(|character| character.is_ascii_alphabetic() && character != 'v')
}

fn numbers(text: &str) -> ([u64; 4], usize) {
    let mut numbers = [0; 4];
    let mut count = 0;
    for part in text.trim_start_matches('v').split('.').take(4) {
        let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
        let Ok(number) = digits.parse() else {
            break;
        };
        numbers[count] = number;
        count += 1;
        if digits.len() != part.len() {
            break;
        }
    }
    (numbers, count)
}

fn increment(mut numbers: [u64; 4], position: usize) -> String {
    numbers[position] = numbers[position].saturating_add(1);
    for number in &mut numbers[position + 1..] {
        *number = 0;
    }
    format!(
        "{}.{}.{}.{}-dev",
        numbers[0], numbers[1], numbers[2], numbers[3]
    )
}

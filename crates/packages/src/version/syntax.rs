//! Expand Composer constraint syntax into primitive comparison terms.
//!
//! `^0.2` becomes `>=0.2.0.0-dev <0.3.0.0-dev`.

use super::{
    Version,
    constraint::{Operator, Term},
};
use anyhow::{Result, bail};

pub(super) fn parse_token(token: &str, terms: &mut Vec<Term>) -> Result<()> {
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
    text.split('+')
        .next()
        .unwrap_or(text)
        .chars()
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

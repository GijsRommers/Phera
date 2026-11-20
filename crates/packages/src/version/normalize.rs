//! Normalize Composer numeric, calendar and branch versions.
//!
//! `2.x-dev` becomes `2.9999999.9999999.9999999-dev`; `dev-main` keeps its branch name.

use anyhow::{Result, bail};
use regex::Regex;
use std::sync::LazyLock;

static CLASSIC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)^v?(\d{1,5})(\.\d+)?(\.\d+)?(\.\d+)?",
        r"[._-]?(?:(stable|beta|b|RC|alpha|a|patch|pl|p)((?:[.-]?\d+)*))?([.-]?dev)?$"
    ))
    .expect("valid version regex")
});
static DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)^v?(\d{4}(?:[.:-]?\d{2}){1,6}(?:[.:-]?\d{1,3}){0,2})",
        r"[._-]?(?:(stable|beta|b|RC|alpha|a|patch|pl|p)((?:[.-]?\d+)*))?([.-]?dev)?$"
    ))
    .expect("valid date version regex")
});
static BRANCH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^v?(\d+)(\.(?:\d+|x|\*))?(\.(?:\d+|x|\*))?(\.(?:\d+|x|\*))?$")
        .expect("valid branch regex")
});

pub fn normalize(text: &str) -> Result<String> {
    let mut text = text.trim();
    if let Some((source, _)) = text.split_once(" as ") {
        text = source;
    }
    if let Some((source, stability)) = text.rsplit_once('@')
        && super::Stability::parse(stability).is_some()
    {
        text = source;
    }
    if matches!(text, "master" | "trunk" | "default") {
        return Ok(format!("dev-{text}"));
    }
    if text
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("dev-"))
    {
        return Ok(format!("dev-{}", &text[4..]));
    }
    if text.chars().any(char::is_whitespace) {
        bail!("Invalid whitespace in Composer version.");
    }
    let text = text.split_once('+').map_or(text, |(version, _)| version);
    if let Some(matches) = CLASSIC.captures(text) {
        let mut numeric = matches[1].to_owned();
        for index in 2..5 {
            numeric.push_str(matches.get(index).map_or(".0", |item| item.as_str()));
        }
        return Ok(suffix(numeric, &matches, 5));
    }
    if let Some(matches) = DATE.captures(text) {
        let numeric = matches[1]
            .chars()
            .map(|character| {
                if character.is_ascii_digit() {
                    character
                } else {
                    '.'
                }
            })
            .collect();
        return Ok(suffix(numeric, &matches, 2));
    }
    if text.to_ascii_lowercase().ends_with("dev") {
        let branch = text[..text.len() - 3].trim_end_matches(['.', '-']);
        if let Some(matches) = BRANCH.captures(branch) {
            let mut numeric = matches[1].to_owned();
            for index in 2..5 {
                let part = matches.get(index).map_or(".x", |item| item.as_str());
                numeric.push_str(&part.replace(['x', 'X', '*'], "9999999"));
            }
            return Ok(format!("{numeric}-dev"));
        }
    }
    bail!("Invalid Composer version {text:?}; use a tag like 1.2.3 or a dev-main branch.")
}

fn suffix(mut numeric: String, matches: &regex::Captures<'_>, index: usize) -> String {
    if let Some(stability) = matches
        .get(index)
        .filter(|value| !value.as_str().is_empty())
    {
        let name = match stability.as_str().to_ascii_lowercase().as_str() {
            "stable" => return numeric,
            "a" | "alpha" => "alpha",
            "b" | "beta" => "beta",
            "rc" => "RC",
            "p" | "pl" | "patch" => "patch",
            _ => "",
        };
        numeric.push('-');
        numeric.push_str(name);
        if let Some(number) = matches.get(index + 1) {
            numeric.push_str(number.as_str().trim_start_matches(['.', '-']));
        }
    }
    if matches
        .get(index + 2)
        .is_some_and(|value| !value.as_str().is_empty())
    {
        numeric.push_str("-dev");
    }
    numeric
}

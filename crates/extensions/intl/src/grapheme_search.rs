//! Search graphemes by their locale's collation weights.
//!
//! ```php
//! grapheme_strpos("aéz", "e\u{0301}"); // 1
//! grapheme_stristr("école", "É");     // "école"
//! ```

use icu_collator::options::{CollatorOptions, Strength};
use icu_collator::{Collator, CollatorBorrowed};
use icu_locale::Locale;
use icu_normalizer::ComposingNormalizer;
use phera_extension_api::{ConstantValue, Context, FunctionEntry, NativeError, NativeResult};
use phera_extension_api::{Param, ParamType, Registry, Value, args};

use crate::{error, grapheme, grapheme_distance};

#[derive(Clone, Copy)]
enum Direction {
    Forward,
    Reverse,
}

#[derive(Clone, Copy)]
enum ResultKind {
    Position(Direction),
    Substring,
}

fn search(
    context: &mut dyn Context,
    arguments: &[Value],
    site: &str,
    kind: ResultKind,
    ignore_case: bool,
) -> NativeResult {
    grapheme::check_locale(arguments, 3, site)?;
    let text = args::bytes(arguments, 0);
    let needle = args::bytes(arguments, 1);
    let offset = args::int(arguments, 2).unwrap_or(0);
    let before = args::bool(arguments, 2).unwrap_or(false);
    let reverse = matches!(kind, ResultKind::Position(Direction::Reverse));
    if !(0..=text.len() as i64).contains(&grapheme::relative(offset, text.len() as i64)) {
        return Err(offset_error(site));
    }
    if text.is_empty() && !needle.is_empty() {
        return Ok(Value::Bool(false));
    }
    if text.is_ascii()
        && !text.windows(2).any(|pair| pair == b"\r\n")
        && (!ignore_case || matches!(kind, ResultKind::Position(_)))
        && (offset >= 0 || reverse || ignore_case)
    {
        let relative_offset = grapheme::relative(offset, text.len() as i64) as usize;
        let start = if reverse && offset < 0 {
            0
        } else {
            relative_offset
        };
        let last = if reverse && offset < 0 {
            relative_offset
        } else {
            text.len()
        };
        let hit = if needle.is_empty() {
            Some(if reverse { last } else { start })
        } else {
            let candidates = text[start..].windows(needle.len()).enumerate();
            let mut matches = candidates.filter_map(|(index, part)| {
                let equal = if ignore_case {
                    part.eq_ignore_ascii_case(needle)
                } else {
                    part == needle
                };
                (equal && start + index <= last).then_some(start + index)
            });
            if reverse {
                matches.next_back()
            } else {
                matches.next()
            }
        };
        if hit.is_some() || needle.is_ascii() {
            return Ok(result(text, hit.map(|at| (at, at)), kind, before));
        }
    }

    if std::str::from_utf8(text).is_err() {
        return grapheme::input_error(context, site, Value::Bool(false));
    }
    if std::str::from_utf8(needle).is_err() {
        return error::fail(
            context,
            site,
            error::INVALID_CHAR,
            "Error converting needle string to UTF-16",
            Value::Bool(false),
        );
    }
    let boundaries: Vec<_> = grapheme::boundaries(text).collect();
    let length = boundaries.len().saturating_sub(1);
    let start = grapheme::relative(offset, length as i64);
    if !(0..=length as i64).contains(&start) {
        return Err(offset_error(site));
    }
    let start = start as usize;
    let position = if needle.is_empty() {
        Some(if reverse && offset >= 0 {
            length
        } else {
            start
        })
    } else {
        let options = args::bytes(arguments, 3);
        let Some(collator) = collator(options, ignore_case) else {
            return error::fail(
                context,
                site,
                error::ILLEGAL_ARGUMENT,
                "Error creating search object",
                Value::Bool(false),
            );
        };
        let high = if reverse && offset < 0 { start } else { length };
        let low = if reverse && offset < 0 { 0 } else { start };
        find(&collator, text, &boundaries, needle, low..=high, reverse)
    };
    Ok(result(
        text,
        position.map(|index| (index, boundaries[index])),
        kind,
        before,
    ))
}

fn offset_error(site: &str) -> NativeError {
    NativeError::value_error(format!(
        "{site}(): Argument #3 ($offset) must be contained in argument #1 ($haystack)"
    ))
}

fn result(text: &[u8], position: Option<(usize, usize)>, kind: ResultKind, before: bool) -> Value {
    let Some((index, byte)) = position else {
        return Value::Bool(false);
    };
    match kind {
        ResultKind::Position(_) => Value::Int(index as i64),
        ResultKind::Substring if before => Value::from(text[..byte].to_vec()),
        ResultKind::Substring => Value::from(text[byte..].to_vec()),
    }
}

pub fn collator(locale: &[u8], ignore_case: bool) -> Option<CollatorBorrowed<'static>> {
    let locale = std::str::from_utf8(locale).ok()?;
    let locale = locale
        .replace('_', "-")
        .parse::<Locale>()
        .unwrap_or(icu_locale::locale!("und"));
    let mut options = CollatorOptions::default();
    if ignore_case {
        options.strength = Some(Strength::Secondary);
    }
    Collator::try_new(locale.into(), options).ok()
}

/// Match consecutive non-ignorable graphemes with a prefix table.
fn find(
    collator: &CollatorBorrowed<'_>,
    text: &[u8],
    boundaries: &[usize],
    needle: &[u8],
    range: std::ops::RangeInclusive<usize>,
    reverse: bool,
) -> Option<usize> {
    let empty = sort_key(collator, &[]);
    let needle: Vec<_> = keys(
        collator,
        needle,
        &grapheme::boundaries(needle).collect::<Vec<_>>(),
    )
    .map(|part| part.key)
    .filter(|key| *key != empty)
    .collect();
    if needle.is_empty() {
        return range
            .into_iter()
            .find(|&index| index + 1 < boundaries.len());
    }
    let mut backtrack = vec![0; needle.len()];
    let mut count = 0;
    for index in 1..needle.len() {
        count = advance(&needle, &backtrack, count, &needle[index]);
        backtrack[index] = count;
    }

    let mut recent = std::collections::VecDeque::with_capacity(needle.len());
    let mut found = None;
    count = 0;
    for part in keys(collator, text, boundaries).filter(|part| part.key != empty) {
        if recent.len() == needle.len() {
            recent.pop_front();
        }
        let ends_grapheme = part.last;
        recent.push_back((part.index, part.first));
        count = advance(&needle, &backtrack, count, &part.key);
        if count == needle.len() {
            if let Some(&(index, true)) = recent.front()
                && ends_grapheme
                && range.contains(&index)
            {
                found = Some(index);
                if !reverse {
                    break;
                }
            }
            count = backtrack[count - 1];
        }
    }
    found
}

fn advance(needle: &[Vec<u8>], table: &[usize], mut count: usize, key: &[u8]) -> usize {
    while count > 0 && needle[count] != key {
        count = table[count - 1];
    }
    count + usize::from(needle[count] == key)
}

struct KeyPart {
    index: usize,
    first: bool,
    last: bool,
    key: Vec<u8>,
}

fn keys<'a>(
    collator: &'a CollatorBorrowed<'_>,
    text: &'a [u8],
    boundaries: &'a [usize],
) -> impl Iterator<Item = KeyPart> + 'a {
    boundaries
        .windows(2)
        .enumerate()
        .flat_map(move |(index, part)| {
            let cluster = &text[part[0]..part[1]];
            let source = std::str::from_utf8(cluster).unwrap_or_default();
            let normalized = ComposingNormalizer::new_nfkc().normalize(source);
            let equivalent = normalized != source && collator.compare(source, &normalized).is_eq();
            let expansion = if equivalent {
                normalized.as_bytes()
            } else {
                cluster
            };
            let mut start = 0;
            let parts: Vec<_> = grapheme::boundaries(expansion)
                .skip(1)
                .map(|end| {
                    let part = KeyPart {
                        index,
                        first: start == 0,
                        last: end == expansion.len(),
                        key: sort_key(collator, &expansion[start..end]),
                    };
                    start = end;
                    part
                })
                .collect();
            parts
        })
}

fn sort_key(collator: &CollatorBorrowed<'_>, text: &[u8]) -> Vec<u8> {
    let mut key = Vec::new();
    let Ok(()) = collator.write_sort_key_utf8_to(text, &mut key);
    key
}

pub fn register(registry: &mut Registry) {
    const HAYSTACK: Param = Param::new("haystack", ParamType::String);
    const NEEDLE: Param = Param::new("needle", ParamType::String);
    const POSITION: &[Param] = &[
        HAYSTACK,
        NEEDLE,
        Param::new("offset", ParamType::Int).with_default(ConstantValue::Int(0)),
        grapheme::LOCALE,
    ];
    const SUBSTRING: &[Param] = &[
        HAYSTACK,
        NEEDLE,
        Param::new("beforeNeedle", ParamType::Bool).with_default(ConstantValue::Bool(false)),
        grapheme::LOCALE,
    ];
    const FORWARD: ResultKind = ResultKind::Position(Direction::Forward);
    const REVERSE: ResultKind = ResultKind::Position(Direction::Reverse);
    for (entry, result) in [
        (
            FunctionEntry::sync("grapheme_strpos", POSITION, |c, a| {
                search(c, a, "grapheme_strpos", FORWARD, false)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("grapheme_stripos", POSITION, |c, a| {
                search(c, a, "grapheme_stripos", FORWARD, true)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("grapheme_strrpos", POSITION, |c, a| {
                search(c, a, "grapheme_strrpos", REVERSE, false)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("grapheme_strripos", POSITION, |c, a| {
                search(c, a, "grapheme_strripos", REVERSE, true)
            }),
            "int|false",
        ),
        (
            FunctionEntry::sync("grapheme_strstr", SUBSTRING, |c, a| {
                search(c, a, "grapheme_strstr", ResultKind::Substring, false)
            }),
            "string|false",
        ),
        (
            FunctionEntry::sync("grapheme_stristr", SUBSTRING, |c, a| {
                search(c, a, "grapheme_stristr", ResultKind::Substring, true)
            }),
            "string|false",
        ),
    ] {
        registry.function(entry);
        registry.describe_function(entry.name, result, None);
    }
    grapheme_distance::register(registry);
}

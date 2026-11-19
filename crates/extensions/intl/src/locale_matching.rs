//! Locale negotiation compares ranges and preserves original language tags.
//!
//! ```php
//! Locale::filterMatches('nl_BE', 'nl');              // true
//! Locale::lookup(['nl', 'fr'], 'nl-BE');             // "nl"
//! Locale::acceptFromHttp('nl-BE, en;q=0.5');         // "nl_BE"
//! ```

use std::collections::HashSet;
use std::sync::OnceLock;

use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use crate::{error, locale, locale_id};

pub fn filter(context: &mut dyn Context, values: &[Value], site: &str) -> NativeResult {
    error::clear(context);
    locale::check_null(values, 0, "languageTag", site)?;
    locale::check_null(values, 1, "locale", site)?;
    let tag = String::from_utf8_lossy(args::bytes(values, 0));
    let range = locale::effective(context, args::bytes(values, 1));
    if range == "*" {
        return Ok(Value::Bool(true));
    }
    if range.len() > locale::MAX_LEN || tag.len() > locale::MAX_LEN {
        return locale::too_long(context, site);
    }
    let canonical = args::bool(values, 2).unwrap_or(false);
    let tag = normalize(&tag, canonical);
    let range = normalize(&range, canonical);
    if canonical && (range.is_empty() || tag.is_empty()) {
        let detail = if range.is_empty() {
            "unable to canonicalize loc_range"
        } else {
            "unable to canonicalize lang_tag"
        };
        return error::fail(context, site, 0, detail, Value::Bool(false));
    }
    let matches = !range.is_empty()
        && tag.strip_prefix(&range).is_some_and(|rest| {
            rest.is_empty() || rest.starts_with('_') || (canonical && rest.starts_with('@'))
        });
    Ok(Value::Bool(matches))
}

pub fn lookup(context: &mut dyn Context, values: &[Value], site: &str) -> NativeResult {
    error::clear(context);
    locale::check_null(values, 1, "locale", site)?;
    locale::check_null(values, 3, "defaultLocale", site)?;
    let fallback = args::string(values, 3);
    let mut range = locale::effective(context, args::bytes(values, 1));
    if args::bytes(values, 1).is_empty()
        && let Some(fallback) = fallback
    {
        range = String::from_utf8_lossy(fallback.as_bytes()).into_owned();
    }
    if range.len() > locale::MAX_LEN {
        return locale::too_long(context, site);
    }
    let Some(tags) = args::array(values, 0).filter(|array| !array.is_empty()) else {
        return Ok(Value::from(""));
    };
    let canonical = args::bool(values, 2).unwrap_or(false);
    let mut candidates = Vec::with_capacity(tags.len().min(100));
    for (_, tag) in tags.iter() {
        let Value::String(tag) = &*tag.deref() else {
            return Err(NativeError::value_error(format!(
                "{site}(): Argument #2 ($locale) must only contain string values"
            )));
        };
        let normalized = normalize(&String::from_utf8_lossy(tag.as_bytes()), canonical);
        if canonical && normalized.is_empty() {
            return lookup_error(context, site, "unable to canonicalize lang_tag");
        }
        candidates.push((normalized, tag.clone()));
    }
    let needle = normalize(&range, canonical);
    if needle.is_empty() {
        return lookup_error(context, site, "unable to canonicalize loc_range");
    }
    for candidate in parents(&needle) {
        if let Some((normalized, original)) = candidates.iter().find(|(tag, _)| tag == candidate) {
            return Ok(if canonical {
                Value::from(normalized.clone())
            } else {
                Value::from(original.clone())
            });
        }
    }
    Ok(fallback.map_or_else(|| Value::from(""), |value| Value::from(value.clone())))
}

fn lookup_error(context: &mut dyn Context, site: &str, detail: &str) -> NativeResult {
    error::fail(context, site, error::ILLEGAL_ARGUMENT, detail, Value::Null)
}

fn normalize(tag: &str, canonical: bool) -> String {
    if canonical {
        locale_id::canonical(tag).to_ascii_lowercase()
    } else {
        tag.replace('-', "_").to_ascii_lowercase()
    }
}

fn parents(tag: &str) -> impl Iterator<Item = &str> {
    std::iter::successors(Some(tag), |tag| {
        let (parent, _) = tag.rsplit_once('_')?;
        if let Some((base, singleton)) = parent.rsplit_once('_')
            && singleton.len() == 1
        {
            Some(base)
        } else {
            Some(parent)
        }
    })
}

fn available() -> &'static HashSet<&'static str> {
    static LOCALES: OnceLock<HashSet<&str>> = OnceLock::new();
    LOCALES.get_or_init(|| include_str!("../data/cldr-locales.txt").lines().collect())
}

pub fn accept(context: &mut dyn Context, values: &[Value], site: &str) -> NativeResult {
    error::clear(context);
    locale::check_null(values, 0, "header", site)?;
    let header = String::from_utf8_lossy(args::bytes(values, 0));
    if header
        .split(',')
        .any(|item| item.len() > locale::MAX_LEN + 1)
    {
        return error::fail(
            context,
            site,
            error::ILLEGAL_ARGUMENT,
            "locale string too long",
            Value::Bool(false),
        );
    }
    let mut choices = Vec::new();
    for item in header
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        let (tag, weight) = match item.split_once(';') {
            Some((tag, option)) => {
                let Some(weight) = quality(option) else {
                    return accept_error(context, site, error::ILLEGAL_ARGUMENT);
                };
                (tag.trim(), weight)
            }
            None => (item, 1000),
        };
        if weight > 0 && tag != "*" {
            choices.push((weight, locale_id::canonical(tag)));
        }
    }
    choices.sort_by_key(|(weight, _)| std::cmp::Reverse(*weight));
    for (_, tag) in &choices {
        if let Some(result) = parents(tag).find(|tag| available().contains(tag)) {
            return Ok(Value::from(result));
        }
    }
    if choices.is_empty() && !header.trim().is_empty() {
        Ok(Value::Bool(false))
    } else {
        accept_error(context, site, 0)
    }
}

fn quality(option: &str) -> Option<u16> {
    let digits = option.trim().strip_prefix("q=")?;
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if fraction.len() > 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let fraction_value = fraction
        .bytes()
        .fold(0_u16, |value, byte| value * 10 + u16::from(byte - b'0'))
        * 10_u16.pow((3 - fraction.len()) as u32);
    match whole {
        "0" => Some(fraction_value),
        "1" if fraction_value == 0 => Some(1000),
        _ => None,
    }
}

fn accept_error(context: &mut dyn Context, site: &str, code: i32) -> NativeResult {
    error::fail(
        context,
        site,
        code,
        "failed to find acceptable locale",
        Value::Bool(false),
    )
}

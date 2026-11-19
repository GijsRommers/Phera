//! PHP accepts ICU locale IDs in addition to BCP 47 language tags.
//!
//! ```php
//! Locale::canonicalize("de-DE-u-co-phonebk"); // "de_DE@collation=phonebook"
//! Locale::parseLocale("en-US-x-Test");       // language, region, private0
//! ```

use std::collections::BTreeMap;

use icu_locale::{Locale, LocaleCanonicalizer, LocaleExpander};

#[derive(Default)]
pub struct LocaleId<'a> {
    pub language: String,
    pub script: String,
    pub region: String,
    pub variants: Vec<String>,
    pub private: Vec<&'a str>,
    pub grandfathered: Option<&'a str>,
}

impl<'a> LocaleId<'a> {
    pub fn parse(input: &'a str) -> Self {
        let base = input.split(['.', '@']).next().unwrap_or_default();
        if grandfathered(base).is_some() {
            return Self {
                language: base.to_owned(),
                grandfathered: Some(base),
                ..Self::default()
            };
        }
        let mut id = Self::default();
        let mut parts = base.split(['_', '-']).peekable();
        let first = parts.next().unwrap_or_default();
        id.language = first.to_ascii_lowercase();
        if id.language == "root" || id.language == "und" {
            id.language.clear();
        }
        let start_private = first.eq_ignore_ascii_case("x");
        if start_private {
            id.private.extend(parts.filter(|part| !part.is_empty()));
            id.language.clear();
            return id;
        }
        if parts.peek().is_some_and(|part| part.len() == 4) {
            id.script = parts.next().unwrap_or_default().to_ascii_lowercase();
            if let Some(first) = id.script.get_mut(..1) {
                first.make_ascii_uppercase();
            }
        }
        match parts.peek() {
            Some(part) if matches!(part.len(), 0 | 2 | 3) => {
                id.region = parts.next().unwrap_or_default().to_ascii_uppercase();
            }
            _ => {}
        }
        let mut extension = false;
        while let Some(part) = parts.next() {
            if part.eq_ignore_ascii_case("x") {
                id.private.extend(parts.filter(|part| !part.is_empty()));
                break;
            }
            extension |= part.len() == 1;
            if !extension && !part.is_empty() {
                id.variants.push(part.to_ascii_uppercase());
            }
        }
        id
    }

    fn without_extensions(&self) -> String {
        let mut result = self.language.clone();
        for part in [&self.script, &self.region] {
            if !part.is_empty() {
                result.push('_');
                result.push_str(part);
            }
        }
        if !self.variants.is_empty() {
            if self.region.is_empty() {
                result.push('_');
            }
            for variant in &self.variants {
                result.push('_');
                result.push_str(variant);
            }
        }
        result
    }
}

pub fn cldr(input: &str) -> Locale {
    let cleaned = input
        .split('.')
        .next()
        .unwrap_or_default()
        .replace('_', "-");
    let parsed = cleaned.parse::<Locale>().or_else(|_| {
        let id = LocaleId::parse(input);
        let base = id.without_extensions().replace('_', "-");
        base.parse::<Locale>().or_else(|_| id.language.parse())
    });
    let mut locale = parsed.unwrap_or(icu_locale::locale!("und"));
    if let Some((_, keywords)) = input.split_once('@') {
        for pair in keywords.split(';') {
            if let Some((key, value)) = pair.split_once('=') {
                set_keyword(&mut locale, key, value);
            }
        }
    }
    LocaleCanonicalizer::new_extended().canonicalize(&mut locale);
    locale
}

fn set_keyword(locale: &mut Locale, name: &str, value: &str) {
    let name = match name {
        "calendar" => "ca",
        "collation" => "co",
        "currency" => "cu",
        "numbers" => "nu",
        "hours" => "hc",
        other => other,
    };
    let value = match value {
        "gregorian" => "gregory",
        "phonebook" => "phonebk",
        "traditional" => "trad",
        other => other,
    };
    if let (Ok(key), Ok(value)) = (name.parse(), value.to_ascii_lowercase().parse()) {
        locale.extensions.unicode.keywords.set(key, value);
    }
}

pub fn canonical(input: &str) -> String {
    let id = LocaleId::parse(input);
    let base = match id.grandfathered.and_then(grandfathered) {
        Some(preferred) if !preferred.is_empty() => {
            LocaleId::parse_without_grandfathered(preferred).without_extensions()
        }
        _ if id.grandfathered.is_some() => {
            LocaleId::parse_without_grandfathered(input).without_extensions()
        }
        _ => id.without_extensions(),
    };
    let modifiers = keywords(input);
    append_keywords(base, &modifiers)
}

impl<'a> LocaleId<'a> {
    fn parse_without_grandfathered(input: &'a str) -> Self {
        let id = Self::parse(input);
        if id.grandfathered.is_none() {
            return id;
        }
        let mut tokens = input.split('-');
        let language = tokens.next().unwrap_or_default().to_ascii_lowercase();
        Self {
            language,
            variants: tokens.map(str::to_ascii_uppercase).collect(),
            ..Self::default()
        }
    }
}

pub fn change_likely(input: &str, minimize: bool) -> String {
    let original = canonical(input);
    let id = LocaleId::parse(&original);
    let mut modern = cldr(&original);
    if modern.id.language.is_unknown() && !id.language.is_empty() {
        return original;
    }
    if minimize {
        LocaleExpander::new_extended().minimize(&mut modern.id);
    } else {
        LocaleExpander::new_extended().maximize(&mut modern.id);
    }
    let base = LocaleId {
        language: if minimize && !id.language.is_empty() {
            id.language
        } else {
            modern.id.language.to_string()
        },
        script: modern
            .id
            .script
            .map_or_else(String::new, |value| value.to_string()),
        region: modern
            .id
            .region
            .map_or_else(String::new, |value| value.to_string()),
        variants: id.variants,
        ..LocaleId::default()
    };
    append_keywords(base.without_extensions(), &keywords(input))
}

pub fn keywords(input: &str) -> BTreeMap<String, String> {
    let (base, explicit) = input.split_once('@').unwrap_or((input, ""));
    let base = base.split('.').next().unwrap_or_default();
    let mut result = BTreeMap::new();
    for pair in explicit.split(';') {
        if let Some((key, value)) = pair.split_once('=') {
            result.insert(key.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let mut parts = base.split(['-', '_']).peekable();
    while let Some(part) = parts.next() {
        if part.len() != 1 || !part.as_bytes()[0].is_ascii_alphanumeric() {
            continue;
        }
        if part.eq_ignore_ascii_case("u") {
            while parts.peek().is_some_and(|item| item.len() > 1) {
                let key = parts.next().unwrap_or_default();
                if key.len() != 2 {
                    continue;
                }
                let mut value = Vec::new();
                while parts.peek().is_some_and(|item| item.len() > 2) {
                    value.push(parts.next().unwrap_or_default());
                }
                let (key, value) = legacy_keyword(key, value.join("-"));
                result.entry(key).or_insert(value);
            }
        } else {
            let mut value = Vec::new();
            while parts
                .peek()
                .is_some_and(|item| item.len() > 1 || part == "x")
            {
                value.push(parts.next().unwrap_or_default());
            }
            if !value.is_empty() && grandfathered(base).is_none() {
                result
                    .entry(part.to_ascii_lowercase())
                    .or_insert_with(|| value.join("-").to_ascii_lowercase());
            }
        }
    }
    result
}

fn legacy_keyword(key: &str, value: String) -> (String, String) {
    let name = match key {
        "ca" => "calendar",
        "co" => "collation",
        "cu" => "currency",
        "nu" => "numbers",
        "hc" => "hours",
        other => other,
    };
    let value = match value.to_ascii_lowercase().as_str() {
        "gregory" => "gregorian".to_owned(),
        "phonebk" => "phonebook".to_owned(),
        "trad" => "traditional".to_owned(),
        "" => "yes".to_owned(),
        _ if key == "cu" => value.to_ascii_uppercase(),
        _ => value.to_ascii_lowercase(),
    };
    (name.to_owned(), value)
}

fn append_keywords(mut base: String, keywords: &BTreeMap<String, String>) -> String {
    for (index, (key, value)) in keywords.iter().enumerate() {
        base.push(if index == 0 { '@' } else { ';' });
        base.push_str(key);
        base.push('=');
        base.push_str(value);
    }
    base
}

fn grandfathered(tag: &str) -> Option<&'static str> {
    include_str!("../data/grandfathered-locales.txt")
        .lines()
        .filter_map(|line| line.split_once(';'))
        .find_map(|(name, preferred)| name.eq_ignore_ascii_case(tag).then_some(preferred))
}

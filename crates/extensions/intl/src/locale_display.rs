//! Display names use the requested display language.
//!
//! ```php
//! Locale::getDisplayRegion('nl_BE', 'fr_FR'); // "Belgique"
//! ```

use icu_experimental::displaynames::{
    DisplayNamesOptions, DisplayNamesPreferences,
    multi::{LanguageDisplayNames, RegionDisplayNames, ScriptDisplayNames, VariantDisplayNames},
};
use icu_locale::Locale;
use phera_extension_api::{Context, NativeResult, Value, args};

use crate::locale_id::{self, LocaleId};
use crate::{error, locale};

pub enum Display {
    Name,
    Language,
    Script,
    Region,
    Variant,
}

impl Display {
    fn part(&self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Language => "language",
            Self::Script => "script",
            Self::Region => "region",
            Self::Variant => "variant",
        }
    }
}

pub fn display(
    context: &mut dyn Context,
    values: &[Value],
    site: &str,
    display: Display,
) -> NativeResult {
    error::clear(context);
    locale::check_null(values, 0, "locale", site)?;
    locale::check_null(values, 1, "displayLocale", site)?;
    let target = locale::effective(context, args::bytes(values, 0));
    let language = locale::effective(context, args::bytes(values, 1));
    if target.len() > locale::MAX_LEN {
        return error::fail(
            context,
            site,
            error::ILLEGAL_ARGUMENT,
            &format!("unable to get locale {}", display.part()),
            Value::Bool(false),
        );
    }
    let target = locale_id::cldr(&target);
    let language = locale_id::cldr(&language);
    let preferences: DisplayNamesPreferences = language.clone().into();
    let options = DisplayNamesOptions::default();
    let result = match display {
        Display::Name => display_name(&target, &language, preferences),
        Display::Language => LanguageDisplayNames::try_new(preferences, options)
            .ok()
            .map(|names| {
                names
                    .of(target.id.language)
                    .map_or_else(|| target.id.language.to_string(), str::to_owned)
            }),
        Display::Script => match target.id.script {
            Some(script) => ScriptDisplayNames::try_new(preferences, options)
                .ok()
                .map(|names| {
                    names
                        .of(script)
                        .map_or_else(|| script.to_string(), str::to_owned)
                }),
            None => Some(String::new()),
        },
        Display::Region => match target.id.region {
            Some(region) => RegionDisplayNames::try_new(preferences, options)
                .ok()
                .map(|names| {
                    names
                        .of(region)
                        .map_or_else(|| region.to_string(), str::to_owned)
                }),
            None => Some(String::new()),
        },
        Display::Variant => VariantDisplayNames::try_new(preferences, options)
            .ok()
            .map(|names| {
                target
                    .id
                    .variants
                    .iter()
                    .map(|&variant| {
                        names
                            .of(variant)
                            .map_or_else(|| variant.to_string().to_ascii_uppercase(), str::to_owned)
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            }),
    };
    let value = result.unwrap_or_else(|| LocaleId::parse(&target.to_string()).language);
    Ok(Value::from(value))
}

fn display_name(
    target: &Locale,
    language: &Locale,
    preferences: DisplayNamesPreferences,
) -> Option<String> {
    let options = DisplayNamesOptions::default();
    let languages = LanguageDisplayNames::try_new(preferences, options).ok()?;
    let name = languages
        .of(target.id.language)
        .unwrap_or(target.id.language.as_str());
    let mut parts = Vec::new();
    if let Some(script) = target.id.script {
        let names = ScriptDisplayNames::try_new(preferences, options).ok()?;
        parts.push(names.of(script).unwrap_or(script.as_str()).to_owned());
    }
    if let Some(region) = target.id.region {
        let names = RegionDisplayNames::try_new(preferences, options).ok()?;
        parts.push(names.of(region).unwrap_or(region.as_str()).to_owned());
    }
    if !target.id.variants.is_empty() {
        let names = VariantDisplayNames::try_new(preferences, options).ok()?;
        parts.extend(target.id.variants.iter().map(|&variant| {
            names
                .of(variant)
                .map_or_else(|| variant.as_str().to_ascii_uppercase(), str::to_owned)
        }));
    }
    let mut parts = parts.into_iter();
    let Some(first) = parts.next() else {
        return Some(name.to_owned());
    };
    let (pattern, separator) = display_patterns(language);
    let qualifiers = parts.fold(first, |left, right| insert(separator, &left, &right));
    Some(insert(pattern, name, &qualifiers))
}

fn insert(pattern: &str, first: &str, second: &str) -> String {
    pattern.replace("{0}", first).replace("{1}", second)
}

fn display_patterns(locale: &Locale) -> (&'static str, &'static str) {
    let id = locale.id.to_string();
    let language = locale.id.language.as_str();
    let patterns = include_str!("../data/locale-display-patterns.tsv");
    for candidate in [id.as_str(), language] {
        if let Some(pattern) = patterns.lines().find_map(|row| {
            let (tag, values) = row.split_once('\t')?;
            (tag == candidate)
                .then(|| values.split_once('\t'))
                .flatten()
        }) {
            return pattern;
        }
    }
    ("{0} ({1})", "{0}, {1}")
}

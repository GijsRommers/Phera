//! PHP exposes the locale's pattern and number symbols separately from formatting.
//!
//! ```php
//! (new NumberFormatter('nl_BE', NumberFormatter::DECIMAL))->getSymbol(0); // ","
//! ```

use std::collections::HashMap;
use std::sync::OnceLock;

use icu_locale::{Locale, LocaleExpander};

pub struct NumberData {
    pub minimum_grouping: i16,
    pub decimal: &'static str,
    pub percent: &'static str,
    pub scientific: &'static str,
    pub currency: &'static str,
    pub accounting: &'static str,
    pub symbols: [&'static str; 12],
}

type LocaleRecords = HashMap<&'static str, HashMap<&'static str, NumberData>>;
static NUMBERS: OnceLock<LocaleRecords> = OnceLock::new();

fn records() -> &'static LocaleRecords {
    NUMBERS.get_or_init(|| {
        let mut locales = LocaleRecords::new();
        for row in include_str!("../data/cldr-number-patterns.tsv").lines() {
            let fields: Vec<_> = row.split('\t').collect();
            let [
                locale,
                system,
                preferred,
                minimum,
                decimal,
                percent,
                scientific,
                currency,
                accounting,
                symbols @ ..,
            ] = fields.as_slice()
            else {
                continue;
            };
            let Ok(symbols) = <[&str; 12]>::try_from(symbols) else {
                continue;
            };
            let values = || NumberData {
                minimum_grouping: minimum.parse().unwrap_or(1),
                decimal,
                percent,
                scientific,
                currency,
                accounting,
                symbols,
            };
            let record = locales.entry(locale).or_default();
            record.insert(system, values());
            if system == preferred {
                record.insert("", values());
            }
        }
        locales
    })
}

static ROOT: NumberData = NumberData {
    minimum_grouping: 1,
    decimal: "#,##0.###",
    percent: "#,##0%",
    scientific: "#E0",
    currency: "¤#,##0.00",
    accounting: "¤#,##0.00;(¤#,##0.00)",
    symbols: [".", ",", ";", "%", "-", "+", ".", "E", "‰", "∞", "NaN", ","],
};

pub fn for_locale(locale: &Locale) -> &'static NumberData {
    let language = locale.id.language.as_str();
    let identifier = locale.id.to_string();
    let options = locale
        .extensions
        .unicode
        .keywords
        .get(&icu_locale::extensions::unicode::key!("nu"));
    let system = options.map(|value| value.to_string()).unwrap_or_default();
    let records = records();
    for candidate in [&identifier, language] {
        if let Some(data) = records
            .get(candidate)
            .and_then(|data| data.get(system.as_str()).or_else(|| data.get("")))
        {
            return data;
        }
    }
    &ROOT
}

pub fn currency(locale: &Locale) -> String {
    if let Some(code) = locale
        .extensions
        .unicode
        .keywords
        .get(&icu_locale::extensions::unicode::key!("cu"))
    {
        return code.to_string().to_ascii_uppercase();
    }
    let mut identifier = locale.id.clone();
    LocaleExpander::new_extended().maximize(&mut identifier);
    let Some(region) = identifier.region else {
        return "XXX".to_owned();
    };
    include_str!("../data/cldr-currency-regions.txt")
        .lines()
        .find_map(|row| {
            let (key, currency) = row.split_once('\t')?;
            (key == region.as_str()).then_some(currency)
        })
        .unwrap_or("XXX")
        .to_owned()
}

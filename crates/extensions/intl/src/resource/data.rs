//! ResourceBundle presents selected compiled CLDR locale data as ICU-style tables.

use std::rc::Rc;

use icu_experimental::displaynames::{
    DisplayNamesOptions,
    multi::{LanguageDisplayNames, RegionDisplayNames, ScriptDisplayNames, VariantDisplayNames},
};
use icu_locale::Locale;
use phera_extension_api::Value;

use crate::number_data;

pub type Table = Rc<Vec<(String, Node)>>;

#[derive(Clone)]
pub enum Node {
    Table(Table),
    Scalar(Value),
}

impl From<&str> for Node {
    fn from(value: &str) -> Self {
        Self::Scalar(Value::from(value))
    }
}

const AVAILABLE: &str = include_str!("../../data/cldr-locales.txt");

pub fn available() -> impl Iterator<Item = &'static str> {
    AVAILABLE.lines()
}

pub fn known(bundle: &str) -> bool {
    matches!(bundle, "ICUDATA" | "ICUDATA-lang" | "ICUDATA-region")
}

pub fn table(values: impl IntoIterator<Item = (&'static str, Node)>) -> Table {
    Rc::new(
        values
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn names(locale: &Locale, kind: &str) -> Table {
    use std::collections::BTreeMap;
    let keys = available().filter_map(|tag| tag.replace('_', "-").parse::<Locale>().ok());
    let preferences = locale.clone().into();
    let options = DisplayNamesOptions::default();
    let mut rows = BTreeMap::new();
    match kind {
        "Languages" => {
            if let Ok(names) = LanguageDisplayNames::try_new(preferences, options) {
                for key in keys {
                    let language = key.id.language;
                    if let Some(value) = names.of(language) {
                        rows.insert(language.to_string(), Node::from(value));
                    }
                }
            }
        }
        "Countries" => {
            if let Ok(names) = RegionDisplayNames::try_new(preferences, options) {
                for key in keys.filter_map(|locale| locale.id.region) {
                    if let Some(value) = names.of(key) {
                        rows.insert(key.to_string(), Node::from(value));
                    }
                }
            }
        }
        "Scripts" => {
            if let Ok(names) = ScriptDisplayNames::try_new(preferences, options) {
                for script in keys.filter_map(|locale| locale.id.script) {
                    if let Some(value) = names.of(script) {
                        rows.insert(script.to_string(), Node::from(value));
                    }
                }
            }
        }
        _ => {
            if let Ok(names) = VariantDisplayNames::try_new(preferences, options) {
                for key in keys {
                    for &variant in key.id.variants.iter() {
                        if let Some(value) = names.of(variant) {
                            rows.insert(
                                variant.to_string().to_ascii_uppercase(),
                                Node::from(value),
                            );
                        }
                    }
                }
            }
        }
    }
    Rc::new(rows.into_iter().collect())
}

pub fn open(locale: &Locale, bundle: &str) -> Table {
    match bundle {
        "ICUDATA-region" => table([("Countries", Node::Table(names(locale, "Countries")))]),
        "ICUDATA-lang" => table(
            ["Languages", "Scripts", "Variants"]
                .map(|name| (name, Node::Table(names(locale, name)))),
        ),
        _ => general(locale),
    }
}

fn general(locale: &Locale) -> Table {
    let lang = locale.id.language.as_str();
    let quotes = match lang {
        "de" => ["„", "“", "‚", "‘"],
        "fr" => ["«", "»", "«", "»"],
        "ja" => ["「", "」", "『", "』"],
        "nl" => ["‘", "’", "“", "”"],
        "ru" => ["«", "»", "„", "“"],
        _ => ["“", "”", "‘", "’"],
    };
    let delimiters = table([
        ("alternateQuotationEnd", Node::from(quotes[3])),
        ("alternateQuotationStart", Node::from(quotes[2])),
        ("quotationEnd", Node::from(quotes[1])),
        ("quotationStart", Node::from(quotes[0])),
    ]);
    let data = number_data::for_locale(locale);
    let patterns = table([
        ("currencyFormat", Node::from(data.currency)),
        ("decimalFormat", Node::from(data.decimal)),
        ("percentFormat", Node::from(data.percent)),
        ("scientificFormat", Node::from(data.scientific)),
    ]);
    let latn = table([("patterns", Node::Table(patterns))]);
    let numbers = table([("default", Node::from("latn")), ("latn", Node::Table(latn))]);
    table([
        ("NumberElements", Node::Table(numbers)),
        ("delimiters", Node::Table(delimiters)),
    ])
}

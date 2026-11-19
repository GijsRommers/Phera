//! PHP internationalization on ICU4X.
//!
//! ```php
//! Normalizer::normalize("e\u{0301}"); // "é"
//! intl_get_error_code();             // U_ZERO_ERROR
//! ```

mod date;
mod error;
mod grapheme;
mod grapheme_distance;
mod grapheme_search;
mod idn;
mod idn_validation;
mod locale;
mod locale_display;
mod locale_id;
mod locale_matching;
mod normalization_data;
mod normalizer;
mod number;
mod number_data;

use phera_extension_api::{ClassEntry, Extension, Registry};

pub struct Intl;

impl Extension for Intl {
    fn name(&self) -> &'static str {
        "intl"
    }

    fn register(&self, registry: &mut Registry) {
        registry.class(ClassEntry {
            parent: Some("Exception"),
            ..ClassEntry::class("IntlException")
        });
        error::register(registry);
        normalizer::register(registry);
        number::register(registry);
        date::register(registry);
        locale::register(registry);
        idn::register(registry);
        grapheme::register(registry);
    }
}

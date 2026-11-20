//! PHP internationalization on ICU4X.
//!
//! ```php
//! Normalizer::normalize("e\u{0301}"); // "é"
//! intl_get_error_code();             // U_ZERO_ERROR
//! ```

mod breaks;
mod calendar;
mod character;
mod collator;
mod date;
mod error;
mod grapheme;
mod grapheme_distance;
mod grapheme_search;
mod idn;
mod idn_validation;
mod iterator;
mod list;
mod locale;
mod locale_display;
mod locale_id;
mod locale_matching;
mod message;
mod normalization_data;
mod normalizer;
mod number;
mod number_data;
mod resource;
mod timezone;
mod transliterator;
mod unsupported;

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
        collator::register(registry);
        list::register(registry);
        message::register(registry);
        transliterator::register(registry);
        iterator::register(registry);
        breaks::register(registry);
        timezone::register(registry);
        calendar::register(registry);
        resource::register(registry);
        character::register(registry);
        unsupported::register(registry);
        grapheme::register(registry);
    }
}

//! PHP internationalization on ICU4X.
//!
//! ```php
//! Normalizer::normalize("e\u{0301}"); // "é"
//! intl_get_error_code();             // U_ZERO_ERROR
//! ```

mod error;
mod grapheme;
mod grapheme_distance;
mod grapheme_search;
mod normalization_data;
mod normalizer;

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
        grapheme::register(registry);
    }
}

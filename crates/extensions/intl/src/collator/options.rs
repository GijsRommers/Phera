//! Collator attributes control strength, case, punctuation and numeric ordering.
//!
//! ```php
//! $collator->setAttribute(Collator::NUMERIC_COLLATION, Collator::ON);
//! $collator->compare("2", "10"); // -1
//! ```

use std::rc::Rc;

use icu_collator::options::{AlternateHandling, CaseLevel, Strength};
use icu_collator::{
    Collator as UnicodeCollator,
    preferences::{CollationCaseFirst, CollationNumericOrdering},
};

use super::Collator;

#[derive(Clone)]
pub struct LegacyOptions {
    french: bool,
    normalization: bool,
}

impl LegacyOptions {
    pub fn new(locale: &str) -> Self {
        Self {
            french: locale.starts_with("fr_CA"),
            normalization: false,
        }
    }
}

pub fn strength_code(strength: Strength) -> i64 {
    match strength {
        Strength::Primary => 0,
        Strength::Secondary => 1,
        Strength::Tertiary => 2,
        Strength::Quaternary => 3,
        Strength::Identical => 15,
        _ => 2,
    }
}

fn strength(code: i64) -> Option<Option<Strength>> {
    Some(match code {
        -1 => None,
        0 => Some(Strength::Primary),
        1 => Some(Strength::Secondary),
        2 => Some(Strength::Tertiary),
        3 => Some(Strength::Quaternary),
        15 => Some(Strength::Identical),
        _ => return None,
    })
}

pub fn get(state: &Collator, attribute: i64) -> Option<i64> {
    let options = state.formatter.resolved_options();
    let on = |enabled| if enabled { 17 } else { 16 };
    Some(match attribute {
        0 => on(state.legacy.french),
        1 => {
            if options.alternate_handling == AlternateHandling::Shifted {
                20
            } else {
                21
            }
        }
        2 => match options.case_first {
            CollationCaseFirst::Lower => 24,
            CollationCaseFirst::Upper => 25,
            _ => 16,
        },
        3 => on(options.case_level == CaseLevel::On),
        4 => on(state.legacy.normalization),
        5 => strength_code(options.strength),
        6 => 16,
        7 => on(options.numeric == CollationNumericOrdering::True),
        _ => return None,
    })
}

pub fn set(state: &mut Collator, attribute: i64, value: i64) -> bool {
    let mut prefs = state.prefs;
    let mut options = state.options;
    match (attribute, value) {
        (0, -1 | 16 | 17) => {
            state.legacy.french = value == 17 || (value == -1 && state.locale.starts_with("fr_CA"));
            return true;
        }
        (1, -1) => options.alternate_handling = None,
        (1, 20) => options.alternate_handling = Some(AlternateHandling::Shifted),
        (1, 21) => options.alternate_handling = Some(AlternateHandling::NonIgnorable),
        (2, -1) => prefs.case_first = None,
        (2, 16) => prefs.case_first = Some(CollationCaseFirst::False),
        (2, 24) => prefs.case_first = Some(CollationCaseFirst::Lower),
        (2, 25) => prefs.case_first = Some(CollationCaseFirst::Upper),
        (3, -1) => options.case_level = None,
        (3, 16) => options.case_level = Some(CaseLevel::Off),
        (3, 17) => options.case_level = Some(CaseLevel::On),
        (4, -1 | 16 | 17) => {
            state.legacy.normalization = value == 17;
            return true;
        }
        (5, _) => {
            let Some(strength) = strength(value) else {
                return false;
            };
            options.strength = strength;
        }
        (6, -1 | 16 | 17) => return true,
        (7, -1) => prefs.numeric_ordering = None,
        (7, 16) => prefs.numeric_ordering = Some(CollationNumericOrdering::False),
        (7, 17) => prefs.numeric_ordering = Some(CollationNumericOrdering::True),
        _ => return false,
    }
    let Ok(formatter) = UnicodeCollator::try_new(prefs, options) else {
        return false;
    };
    state.prefs = prefs;
    state.options = options;
    state.formatter = Rc::new(formatter);
    true
}

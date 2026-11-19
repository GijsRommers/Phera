//! A parent access needs a parent: `class C { function f() { parent::run(); } }`.
//! Identifier: `class.noParent`; level 0.

use super::{Finding, Rule};

pub struct NoParent;

impl Rule for NoParent {
    fn identifier(&self) -> &'static str {
        "class.noParent"
    }

    fn level(&self) -> u8 {
        0
    }

    fn description(&self) -> &'static str {
        "A parent access needs a parent class."
    }

    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::NoParent(class) => Some(format!(
                "{class} uses parent but has no known parent class."
            )),
            _ => None,
        }
    }
}

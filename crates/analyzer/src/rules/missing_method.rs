//! Concrete classes must implement inherited contracts.
//! Identifier: `class.missingMethod`; level 0. PHP example: `class A implements Countable {}`.

use super::{Finding, Rule};

pub struct MissingMethod;

impl Rule for MissingMethod {
    fn identifier(&self) -> &'static str {
        "class.missingMethod"
    }
    fn level(&self) -> u8 {
        0
    }
    fn description(&self) -> &'static str {
        "Concrete classes must implement inherited contracts."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::MissingMethod(class, name) => {
                Some(format!("Class {class} must implement method {name}()."))
            }
            _ => None,
        }
    }
}

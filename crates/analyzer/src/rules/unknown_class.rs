//! Referenced classes must exist.
//! Identifier: `class.notFound`; level 0. PHP example: `new Missing;`.

use super::{Finding, Rule};

pub struct UnknownClass;

impl Rule for UnknownClass {
    fn identifier(&self) -> &'static str {
        "class.notFound"
    }
    fn level(&self) -> u8 {
        0
    }
    fn description(&self) -> &'static str {
        "Referenced classes must exist."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::UnknownClass(name) => Some(format!("Class {name} not found.")),
            _ => None,
        }
    }
}

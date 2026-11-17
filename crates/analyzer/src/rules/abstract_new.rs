//! Abstract classes and interfaces cannot be instantiated.
//! Identifier: `new.abstract`; level 0. PHP example: `abstract class A {} new A;`.

use super::{Finding, Rule};

pub struct AbstractNew;

impl Rule for AbstractNew {
    fn identifier(&self) -> &'static str {
        "new.abstract"
    }
    fn level(&self) -> u8 {
        0
    }
    fn description(&self) -> &'static str {
        "Abstract classes and interfaces cannot be instantiated."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::AbstractNew(name) => Some(format!(
                "Cannot instantiate abstract class or interface {name}."
            )),
            _ => None,
        }
    }
}

//! Interfaces cannot be instantiated.
//! Identifier: `new.interface`; level 0. PHP example: `new Iterator;`.

use super::{Finding, Rule};

pub struct InterfaceNew;

impl Rule for InterfaceNew {
    fn identifier(&self) -> &'static str {
        "new.interface"
    }
    fn level(&self) -> u8 {
        0
    }
    fn description(&self) -> &'static str {
        "Interfaces cannot be instantiated."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::InterfaceNew(name) => Some(format!("Cannot instantiate interface {name}.")),
            _ => None,
        }
    }
}

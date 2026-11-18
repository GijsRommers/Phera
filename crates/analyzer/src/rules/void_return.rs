//! Void functions cannot return a value.
//! Identifier: `return.void`; level 1. PHP example: `function f(): void { return 1; }`.

use super::{Finding, Rule};

pub struct VoidReturn;

impl Rule for VoidReturn {
    fn identifier(&self) -> &'static str {
        "return.void"
    }
    fn level(&self) -> u8 {
        1
    }
    fn description(&self) -> &'static str {
        "Void functions cannot return a value."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::VoidReturn => Some("A function with return type void returns a value.".into()),
            _ => None,
        }
    }
}

//! The current object exists only in object contexts.
//! Identifier: `variable.undefined`; level 0. PHP example: `function f() { echo $this; }`.

use super::{Finding, Rule};

pub struct ThisContext;

impl Rule for ThisContext {
    fn identifier(&self) -> &'static str {
        "variable.undefined"
    }
    fn level(&self) -> u8 {
        0
    }
    fn description(&self) -> &'static str {
        "The current object exists only in object contexts."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::ThisContext => Some("Using $this outside an object context.".into()),
            _ => None,
        }
    }
}

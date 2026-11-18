//! Calls must satisfy known parameter counts.
//! Identifier: `arguments.count`; level 0. PHP example: `strlen();`.

use super::{Finding, Rule};

pub struct ArgumentCount;

impl Rule for ArgumentCount {
    fn identifier(&self) -> &'static str {
        "arguments.count"
    }
    fn level(&self) -> u8 {
        0
    }
    fn description(&self) -> &'static str {
        "Calls must satisfy known parameter counts."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::ArgumentCount(message) => Some(message.clone()),
            _ => None,
        }
    }
}

//! Called functions must exist.
//! Identifier: `function.notFound`; level 0. PHP example: `missing();`.

use super::{Finding, Rule};

pub struct UnknownFunction;

impl Rule for UnknownFunction {
    fn identifier(&self) -> &'static str {
        "function.notFound"
    }
    fn level(&self) -> u8 {
        0
    }
    fn description(&self) -> &'static str {
        "Called functions must exist."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::UnknownFunction(name) => Some(format!("Function {name} not found.")),
            _ => None,
        }
    }
}

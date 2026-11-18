//! Read variables must have been defined.
//! Identifier: `variable.undefined`; level 0. PHP example: `echo $missing;`.

use super::{Finding, Rule};

pub struct UndefinedVariable;

impl Rule for UndefinedVariable {
    fn identifier(&self) -> &'static str {
        "variable.undefined"
    }
    fn level(&self) -> u8 {
        0
    }
    fn description(&self) -> &'static str {
        "Read variables must have been defined."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::UndefinedVariable(name) => Some(format!("Undefined variable: ${name}")),
            _ => None,
        }
    }
}

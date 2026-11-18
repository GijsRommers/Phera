//! Variables read after branches must exist on every path.
//! Identifier: `variable.undefined`; level 1. PHP example: `if ($flag) { $x = 1; } echo $x;`.

use super::{Finding, Rule};

pub struct PossibleVariable;

impl Rule for PossibleVariable {
    fn identifier(&self) -> &'static str {
        "variable.undefined"
    }
    fn level(&self) -> u8 {
        1
    }
    fn description(&self) -> &'static str {
        "Variables read after branches must exist on every path."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::PossibleVariable(name) => {
                Some(format!("Variable ${name} might not be defined."))
            }
            _ => None,
        }
    }
}

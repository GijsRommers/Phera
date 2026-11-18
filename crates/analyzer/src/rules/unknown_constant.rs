//! Referenced constants must exist.
//! Identifier: `constant.notFound`; level 0. PHP example: `echo MISSING;`.

use super::{Finding, Rule};

pub struct UnknownConstant;

impl Rule for UnknownConstant {
    fn identifier(&self) -> &'static str {
        "constant.notFound"
    }
    fn level(&self) -> u8 {
        0
    }
    fn description(&self) -> &'static str {
        "Referenced constants must exist."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::UnknownConstant(name) => Some(format!("Constant {name} not found.")),
            _ => None,
        }
    }
}

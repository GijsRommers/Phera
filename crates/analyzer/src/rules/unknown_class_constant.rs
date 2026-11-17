//! Referenced constants must exist.
//! Identifier: `classConstant.notFound`; level 0. PHP example: `A::MISSING;`.

use super::{Finding, Rule};

pub struct UnknownClassConstant;

impl Rule for UnknownClassConstant {
    fn identifier(&self) -> &'static str {
        "classConstant.notFound"
    }
    fn level(&self) -> u8 {
        0
    }
    fn description(&self) -> &'static str {
        "Referenced constants must exist."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::UnknownClassConstant(name) => Some(format!("Constant {name} not found.")),
            _ => None,
        }
    }
}

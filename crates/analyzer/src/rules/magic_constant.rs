//! Magic constants must use a PHP spelling.
//! Identifier: `constant.notFound`; level 1. PHP example: `echo __MISSING__;`.

use super::{Finding, Rule};

pub struct MagicConstant;

impl Rule for MagicConstant {
    fn identifier(&self) -> &'static str {
        "constant.notFound"
    }
    fn level(&self) -> u8 {
        1
    }
    fn description(&self) -> &'static str {
        "Magic constants must use a PHP spelling."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::MagicConstant(name) => Some(format!("Unknown magic constant {name}.")),
            _ => None,
        }
    }
}

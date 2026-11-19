//! Declared value returns must cover every reachable path.
//! Identifier: `return.missing`; fallthrough is reported at level 0. PHP example: `function f(): int {}`.

use super::{Finding, Rule};

pub struct MissingReturn;

impl Rule for MissingReturn {
    fn identifier(&self) -> &'static str {
        "return.missing"
    }
    fn level(&self) -> u8 {
        0
    }
    fn description(&self) -> &'static str {
        "Declared value returns must cover every reachable path."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::MissingReturn(name) => Some(format!(
                "{name} should return a value but a path does not return or throw."
            )),
            _ => None,
        }
    }
}

//! Named arguments must name accepted parameters.
//! Identifier: `argument.unknown`; level 0. PHP example: `strlen(wrong: "x");`.

use super::{Finding, Rule};

pub struct NamedArgument;

impl Rule for NamedArgument {
    fn identifier(&self) -> &'static str {
        "argument.unknown"
    }
    fn level(&self) -> u8 {
        0
    }
    fn description(&self) -> &'static str {
        "Named arguments must name accepted parameters."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::NamedArgument(call, name) => {
                Some(format!("Unknown parameter ${name} in call to {call}."))
            }
            _ => None,
        }
    }
}

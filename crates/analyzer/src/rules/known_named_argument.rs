//! Named arguments must name accepted parameters.
//! Identifier: `argument.unknown`; level 1. PHP example: `$f = fn($x) => $x; $f(wrong: 1);`.

use super::{Finding, Rule};

pub struct KnownNamedArgument;

impl Rule for KnownNamedArgument {
    fn identifier(&self) -> &'static str {
        "argument.unknown"
    }
    fn level(&self) -> u8 {
        1
    }
    fn description(&self) -> &'static str {
        "Named arguments must name accepted parameters."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::KnownNamedArgument(call, name) => {
                Some(format!("Unknown parameter ${name} in call to {call}."))
            }
            _ => None,
        }
    }
}

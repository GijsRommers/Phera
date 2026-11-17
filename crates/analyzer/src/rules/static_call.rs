//! Static calls require static methods.
//! Identifier: `method.staticCall`; level 0. PHP example: `A::instanceMethod();`.

use super::{Finding, Rule};

pub struct StaticCall;

impl Rule for StaticCall {
    fn identifier(&self) -> &'static str {
        "method.staticCall"
    }
    fn level(&self) -> u8 {
        0
    }
    fn description(&self) -> &'static str {
        "Static calls require static methods."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::StaticCall(class, name) => {
                Some(format!("Static call to instance method {class}::{name}()."))
            }
            _ => None,
        }
    }
}

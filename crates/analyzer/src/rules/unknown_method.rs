//! Known classes must declare called methods.
//! Identifier: `method.notFound`; level 0. PHP example: `$this->missing();`.

use super::{Finding, Rule};

pub struct UnknownMethod;

impl Rule for UnknownMethod {
    fn identifier(&self) -> &'static str {
        "method.notFound"
    }
    fn level(&self) -> u8 {
        0
    }
    fn description(&self) -> &'static str {
        "Known classes must declare called methods."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::UnknownMethod(class, name) => {
                Some(format!("Call to undefined method {class}::{name}()."))
            }
            _ => None,
        }
    }
}

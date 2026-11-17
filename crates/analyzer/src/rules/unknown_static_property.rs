//! Known classes must declare accessed properties.
//! Identifier: `staticProperty.notFound`; level 0. PHP example: `A::$missing;`.

use super::{Finding, Rule};

pub struct UnknownStaticProperty;

impl Rule for UnknownStaticProperty {
    fn identifier(&self) -> &'static str {
        "staticProperty.notFound"
    }
    fn level(&self) -> u8 {
        0
    }
    fn description(&self) -> &'static str {
        "Known classes must declare accessed properties."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::UnknownStaticProperty(class, name) => {
                Some(format!("Access to undefined property {class}::${name}."))
            }
            _ => None,
        }
    }
}

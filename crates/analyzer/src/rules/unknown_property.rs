//! Known classes must declare accessed properties.
//! Identifier: `property.notFound`; level 0. PHP example: `$this->missing;`.

use super::{Finding, Rule};

pub struct UnknownProperty;

impl Rule for UnknownProperty {
    fn identifier(&self) -> &'static str {
        "property.notFound"
    }
    fn level(&self) -> u8 {
        0
    }
    fn description(&self) -> &'static str {
        "Known classes must declare accessed properties."
    }
    fn message(&self, finding: &Finding) -> Option<String> {
        match finding {
            Finding::UnknownProperty(class, name) => {
                Some(format!("Access to undefined property {class}::${name}."))
            }
            _ => None,
        }
    }
}

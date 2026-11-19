//! Attributes also name classes: `#[AttributeName(Foo::VALUE)] function f() {}`.

use super::*;

impl Analyzer<'_> {
    pub fn check_attributes(&mut self, attributes: &[Attribute], scope: &mut Scope) {
        for attribute in attributes {
            self.class_name(&attribute.name, true);
            for argument in &attribute.arguments {
                self.expression(&argument.value, scope, false);
            }
        }
    }
}

//! `class Base { public static function make() { return new static(); } }`
//! can call a subclass constructor with incompatible parameters.

use super::*;

impl Analyzer<'_> {
    pub(super) fn check_late_static_new(&mut self, class: &str, span: Span) {
        let Some(info) = self.index.class(class) else {
            return;
        };
        if info.modifiers.is_final
            || info.documented.final_class
            || info.kind == ClassKind::Trait
            || !self.index.complete(class)
        {
            return;
        }
        let has_constructor_contract = self.index.ancestry(class).iter().any(|owner| {
            owner.documented.consistent_constructor
                || owner.methods.get("__construct").is_some_and(|method| {
                    method.modifiers.is_final
                        || method.documented_final
                        || method.abstract_body
                        || method.modifiers.is_abstract
                        || owner.kind == ClassKind::Interface
                })
        });
        if !has_constructor_contract {
            self.diagnose("new.static", span, "Unsafe usage of new static().".into());
        }
    }
}

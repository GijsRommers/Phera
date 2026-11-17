//! Known member lookup: `$this->missing` is checked against the class index.
//! Unknown facts stay silent; no PHP code is loaded or executed.

use super::*;

impl Analyzer<'_> {
    pub fn property(
        &mut self,
        ty: &ValueType,
        name: &MemberName,
        scope: &mut Scope,
        span: Span,
        quiet: bool,
        access: Access,
    ) -> ValueType {
        if let (ValueType::Class(class), MemberName::Identifier(name)) = (ty, name) {
            self.named_property(
                class,
                &name.name,
                span,
                quiet || !access.reports_missing(),
                PropertyAccess::Instance,
            )
        } else {
            if let MemberName::Expression(e) = name {
                self.expression(e, scope, false);
            }
            ValueType::Mixed
        }
    }

    pub fn named_property(
        &mut self,
        class: &str,
        name: &str,
        span: Span,
        quiet: bool,
        access: PropertyAccess,
    ) -> ValueType {
        if let Some(property) = self.index.property(class, name) {
            self.relative_type(property.ty.clone(), class)
        } else {
            if !quiet
                && self.index.complete(class)
                && self
                    .index
                    .class(class)
                    .is_some_and(|c| c.kind != ClassKind::Trait)
                && !self.index.open_properties(class)
            {
                let finding = match access {
                    PropertyAccess::Instance => Finding::UnknownProperty(class.into(), name.into()),
                    PropertyAccess::Static => {
                        Finding::UnknownStaticProperty(class.into(), name.into())
                    }
                };
                self.report(finding, span);
            }
            ValueType::Mixed
        }
    }

    pub fn relative_type(&self, ty: ValueType, class: &str) -> ValueType {
        match &ty {
            ValueType::Class(name) if matches!(name.to_lowercase().as_str(), "self" | "static") => {
                ValueType::Class(class.into())
            }
            _ => ty,
        }
    }
}

//! A `Readable&Seekable` receiver can use both contracts.
//! Levels through 5 accept a member available on at least one union arm.

use super::{member_checks::receiver_name, *};
use crate::types::{Field, Shape, ShapeKey, ShapeKind};

#[derive(Default)]
pub struct Receivers<'a> {
    pub classes: Vec<&'a str>,
    pub known_scalar: bool,
    pub uncertain: bool,
}

impl<'a> Receivers<'a> {
    pub fn of(ty: &'a ValueType) -> Self {
        let mut receivers = Self::default();
        let mut pending = vec![ty];
        while let Some(ty) = pending.pop() {
            match ty {
                ValueType::Class(name) => receivers.classes.push(name),
                ValueType::Generic(generic) => receivers.classes.push(&generic.base),
                ValueType::Callable(_) => receivers.classes.push("Closure"),
                ValueType::Union(types) => pending.extend(types.iter()),
                ValueType::Intersection(types) => pending.extend(types.iter().filter(
                    |ty| !matches!(ty, ValueType::Shape(shape) if shape.kind == ShapeKind::Object),
                )),
                ValueType::Template(template) => pending.push(&template.bound),
                ValueType::Conditional(conditional) => {
                    pending.extend([&conditional.yes, &conditional.no])
                }
                ValueType::Bool
                | ValueType::BoolLiteral(_)
                | ValueType::Int
                | ValueType::IntLiteral(_)
                | ValueType::IntRange(..)
                | ValueType::OverflowingInt
                | ValueType::Float
                | ValueType::String
                | ValueType::StringLiteral(_)
                | ValueType::StringRefinement(_)
                | ValueType::ClassString(_)
                | ValueType::Array
                | ValueType::Resource => receivers.known_scalar = true,
                ValueType::Collection(collection)
                    if collection.kind != crate::types::CollectionKind::Iterable =>
                {
                    receivers.known_scalar = true;
                }
                ValueType::Shape(shape) if shape.kind != crate::types::ShapeKind::Object => {
                    receivers.known_scalar = true;
                }
                // Nullable receivers are checked at level 8. Keep the type
                // alternatives that can contribute a method at lower levels.
                ValueType::Null => {}
                _ => receivers.uncertain = true,
            }
        }
        receivers
    }
}

impl Analyzer<'_> {
    pub fn method_receiver(
        &mut self,
        ty: &ValueType,
        method: &str,
        span: Span,
        access: Access,
    ) -> Option<String> {
        if let ValueType::Class(name) = ty {
            return Some(name.clone());
        }
        if self.level < 2 {
            return None;
        }
        if self.strict_receiver(ty) && !matches!(access, Access::Guarded | Access::GuardedStatic) {
            let receiver = receiver_name(ty);
            self.diagnose(
                "method.nonObject",
                span,
                format!("Cannot call method {method}() on {receiver}."),
            );
            return None;
        }
        if self.missing_method_arm(ty, method, access) {
            let receiver = receiver_name(ty);
            self.diagnose(
                "method.notFound",
                span,
                format!("Call to an undefined method {receiver}::{method}()."),
            );
            return None;
        }
        let receivers = Receivers::of(ty);
        if receivers.classes.is_empty()
            && !receivers.uncertain
            && (receivers.known_scalar || *ty == ValueType::Null)
        {
            self.diagnose(
                "method.nonObject",
                span,
                format!("Cannot call method {method}() on {ty}."),
            );
            return None;
        }
        let mut reports_missing = !receivers.classes.is_empty() && !receivers.uncertain;
        let mut selected = None;
        for class in receivers.classes {
            self.dependencies.resolve(self.index, class);
            if self
                .index
                .method(class, method)
                .or_else(|| self.index.doc_method(class, method))
                .is_some()
                && selected.replace(class).is_some()
            {
                // Do not apply a single signature to several possible receivers.
                return None;
            }
            reports_missing &= self.can_report_missing_method(class, method, access);
        }
        if let Some(class) = selected {
            return Some(class.into());
        }
        if reports_missing {
            self.report(Finding::UnknownMethod(ty.to_string(), method.into()), span);
        }
        None
    }

    pub fn property_receiver(
        &mut self,
        ty: &ValueType,
        name: &str,
        span: Span,
        quiet: bool,
    ) -> ValueType {
        if let ValueType::Intersection(components) = ty
            && let Some(field) = components.iter().find_map(|ty| match ty {
                ValueType::Shape(shape) if shape.kind == ShapeKind::Object => {
                    object_field(shape, name).filter(|field| !field.optional)
                }
                _ => None,
            })
        {
            return field.value.clone();
        }
        if let ValueType::Shape(shape) = ty
            && shape.kind == crate::types::ShapeKind::Object
        {
            if let Some(field) = object_field(shape, name) {
                return field.value.clone();
            }
            if !quiet && shape.remainder.is_none() {
                self.report(Finding::UnknownProperty(ty.to_string(), name.into()), span);
            }
            return ValueType::Mixed;
        }
        if !quiet && self.strict_receiver(ty) {
            let receiver = receiver_name(ty);
            self.diagnose(
                "property.nonObject",
                span,
                format!("Cannot access property ${name} on {receiver}."),
            );
            return ValueType::Mixed;
        }
        if !quiet && self.missing_property_arm(ty, name) {
            let receiver = receiver_name(ty);
            self.diagnose(
                "property.notFound",
                span,
                format!("Access to an undefined property {receiver}::${name}."),
            );
            return ValueType::Mixed;
        }
        let receivers = Receivers::of(ty);
        if receivers.classes.is_empty()
            && !receivers.uncertain
            && (receivers.known_scalar || *ty == ValueType::Null)
            && !quiet
        {
            self.diagnose(
                "property.nonObject",
                span,
                format!("Cannot access property ${name} on {ty}."),
            );
            return ValueType::Mixed;
        }
        let mut missing = !receivers.classes.is_empty() && !receivers.uncertain;
        let mut ty_result = ValueType::Never;
        for class in receivers.classes {
            self.dependencies.resolve(self.index, class);
            if let Some(property) = self.index.property(class, name) {
                ty_result =
                    ty_result.join(&self.relative_type(property.effective_type(self.index), class));
                missing = false;
            } else if let Some(ty) = self.index.doc_property_type(class, name) {
                ty_result = ty_result.join(&self.relative_type(ty.clone(), class));
                missing = false;
            } else {
                missing &= self.can_report_missing_property(class, name, PropertyAccess::Instance);
            }
        }
        if missing && !quiet {
            self.report(Finding::UnknownProperty(ty.to_string(), name.into()), span);
        }
        if ty_result == ValueType::Never || receivers.uncertain {
            ValueType::Mixed
        } else {
            ty_result
        }
    }
}

fn object_field<'a>(shape: &'a Shape, name: &str) -> Option<&'a Field> {
    shape
        .fields
        .iter()
        .find(|field| matches!(&field.key, ShapeKey::String(key) if key.as_ref() == name))
}

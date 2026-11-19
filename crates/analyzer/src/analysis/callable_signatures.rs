//! Callable objects have method signatures: `(new C)(wrong: 1)` uses `C::__invoke`.
//! Ordinary closures also expose their actual parameters through `->__invoke()`.

use super::*;
use crate::index::Signature;

impl Analyzer<'_> {
    pub fn callable_signature(&mut self, ty: ValueType) -> Option<Signature> {
        match ty {
            ValueType::Callable(signature) => Some((*signature).clone()),
            // A generic Closure type has no known engine-generated parameter list.
            ValueType::Class(class) if class.eq_ignore_ascii_case("Closure") => None,
            ValueType::Class(class) => {
                self.dependencies.resolve(self.index, &class);
                let mut signature = self.index.method(&class, "__invoke")?.clone();
                signature.returns = self.relative_type(signature.returns, &class);
                Some(signature)
            }
            ValueType::StringLiteral(bytes) if self.level >= 5 => {
                let name = std::str::from_utf8(&bytes).ok()?.trim_start_matches('\\');
                if let Some((class, method)) = name.split_once("::") {
                    self.callable_method(class, method)
                } else {
                    self.index
                        .functions
                        .get(&name.to_ascii_lowercase())
                        .map(|signature| signature.clone().typed(self.index))
                }
            }
            ValueType::Shape(shape) if self.level >= 5 && shape.fields.len() == 2 => {
                let owner = shape.fields.iter().find(|field| {
                    field.key == crate::types::ShapeKey::Integer(0) && !field.optional
                })?;
                let method = shape.fields.iter().find(|field| {
                    field.key == crate::types::ShapeKey::Integer(1) && !field.optional
                })?;
                let ValueType::StringLiteral(method) = &method.value else {
                    return None;
                };
                let class = match &owner.value {
                    ValueType::ClassString(bound) => bound.class_name()?,
                    ValueType::StringLiteral(name) => {
                        std::str::from_utf8(name).ok()?.trim_start_matches('\\')
                    }
                    given => given.class_name()?,
                };
                self.callable_method(class, std::str::from_utf8(method).ok()?)
            }
            _ => None,
        }
    }

    fn callable_method(&mut self, class: &str, method: &str) -> Option<Signature> {
        self.dependencies.resolve(self.index, class);
        let (owner, signature) = self.index.method_owner(class, method)?;
        Some(signature.clone().typed(self.index).for_receiver(
            self.index,
            owner,
            &ValueType::Class(class.into()),
        ))
    }

    pub fn invoke_closure(
        &mut self,
        signature: &Signature,
        arguments: &Arguments,
        scope: &mut Scope,
        span: Span,
    ) -> ValueType {
        if matches!(arguments, Arguments::FirstClassCallable) {
            return signature.clone().as_closure();
        }
        if self.arguments(
            arguments,
            Some(signature),
            "Closure::__invoke()",
            scope,
            span,
            CallKind::Callable,
        ) == Evaluation::Never
        {
            return ValueType::Never;
        }
        self.unbound_invocation(signature);
        self.apply_global_effects(Some(signature), scope);
        signature.returns.clone()
    }
}

impl Signature {
    pub fn as_closure(mut self) -> ValueType {
        self.closure_object = true;
        ValueType::Callable(std::sync::Arc::new(self))
    }
}

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
            _ => None,
        }
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

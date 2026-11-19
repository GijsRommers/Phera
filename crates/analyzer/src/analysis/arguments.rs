//! Argument binding: `f(b: 2, a: 1)` supplies parameters by name.
//! Unknown facts stay silent; no PHP code is loaded or executed.

use super::*;
use crate::index::Signature;
use std::collections::BTreeSet;

impl Analyzer<'_> {
    pub fn arguments(
        &mut self,
        arguments: &Arguments,
        signature: Option<&Signature>,
        label: &str,
        scope: &mut Scope,
        span: Span,
        kind: CallKind,
    ) -> Evaluation {
        let Arguments::List(arguments) = arguments else {
            return Evaluation::Completes;
        };
        if self.evaluate_arguments(arguments, signature, scope) == Evaluation::Never {
            return Evaluation::Never;
        }
        scope.forget_reference_types();
        self.record_exception(scope);
        let Some(signature) = signature else {
            return Evaluation::Completes;
        };
        let supplied = self.bind_arguments(arguments, signature, label, span, kind);
        if !arguments.iter().any(|a| a.spread) {
            self.check_arity(arguments.len(), &supplied, signature, label, span);
        }
        Evaluation::Completes
    }

    fn evaluate_arguments(
        &mut self,
        arguments: &[Argument],
        signature: Option<&Signature>,
        scope: &mut Scope,
    ) -> Evaluation {
        for (position, argument) in arguments.iter().enumerate() {
            let parameter = signature.and_then(|s| {
                if let Some(name) = &argument.name {
                    s.parameters
                        .iter()
                        .find(|p| p.name == name.name)
                        .or_else(|| s.parameters.last().filter(|p| p.variadic))
                } else {
                    s.parameters
                        .get(position)
                        .or_else(|| s.parameters.last().filter(|p| p.variadic))
                }
            });
            if parameter.is_some_and(|p| p.by_reference) {
                self.ensure_reference(&argument.value, scope);
            }
            let previous_binding = self.pending_binding.clone();
            if let Some(class) = parameter.and_then(|p| p.closure_receiver.as_deref()) {
                self.prepare_documented_binding(&argument.value, class);
            }
            let ty = self.expression(&argument.value, scope, signature.is_none());
            let explicitly_bound = self
                .pending_binding
                .as_ref()
                .is_some_and(|(offset, _)| *offset == argument.value.span.start);
            self.pending_binding = previous_binding;
            if !explicitly_bound
                && closure_binding::is_closure(&argument.value)
                && let ValueType::Callable(signature) = &ty
            {
                self.unbound_invocation(signature);
            }
            if ty == ValueType::Never {
                return Evaluation::Never;
            }
            if self.context.scope_kind == ScopeKind::Global
                && (ty.may_be_callable() || parameter.is_some_and(|p| p.ty.may_be_callable()))
            {
                scope.unknown_writes();
            }
            if (signature.is_none() || parameter.is_some_and(|p| p.by_reference))
                && let ExpressionKind::Variable(Variable::Named(name)) = &argument.value.kind
            {
                scope.external_references.insert(name.clone());
                let definition = scope.get(name).definition;
                scope.define(name, ValueType::Mixed);
                if signature.is_none()
                    && definition != Definition::Definite
                    && let Some(variable) = scope.variables.get_mut(name)
                {
                    variable.definition = Definition::Unknown;
                }
            }
        }
        Evaluation::Completes
    }

    fn bind_arguments(
        &mut self,
        arguments: &[Argument],
        signature: &Signature,
        label: &str,
        span: Span,
        kind: CallKind,
    ) -> BTreeSet<usize> {
        let mut supplied = BTreeSet::new();
        let variadic = signature.parameters.iter().any(|p| p.variadic);
        for (position, argument) in arguments.iter().enumerate() {
            if let Some(name) = &argument.name {
                if let Some(index) = signature
                    .parameters
                    .iter()
                    .position(|p| p.name == name.name)
                {
                    supplied.insert(index);
                } else if !variadic {
                    let finding = match kind {
                        CallKind::Direct => Finding::NamedArgument(label.into(), name.name.clone()),
                        CallKind::Callable => {
                            Finding::KnownNamedArgument(label.into(), name.name.clone())
                        }
                    };
                    self.report(finding, span);
                }
            } else if !argument.spread {
                supplied.insert(position);
            }
        }
        supplied
    }

    fn check_arity(
        &mut self,
        count: usize,
        supplied: &BTreeSet<usize>,
        signature: &Signature,
        label: &str,
        span: Span,
    ) {
        let missing = signature
            .parameters
            .iter()
            .enumerate()
            .any(|(i, p)| !p.optional && !p.variadic && !supplied.contains(&i));
        let variadic = signature.parameters.iter().any(|p| p.variadic);
        let excessive = !variadic && !signature.accepts_extra && count > signature.parameters.len();
        if missing || excessive {
            let required = signature
                .parameters
                .iter()
                .filter(|p| !p.optional && !p.variadic)
                .count();
            self.report(
                Finding::ArgumentCount(format!(
                    "{label} called with {count} arguments; \
                {required} required, {} accepted{}.",
                    signature.parameters.len(),
                    if variadic { " or more" } else { "" }
                )),
                span,
            );
        }
    }
}

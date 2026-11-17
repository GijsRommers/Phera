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
    ) {
        let Arguments::List(arguments) = arguments else {
            return;
        };
        self.evaluate_arguments(arguments, signature, scope);
        self.record_exception(scope);
        let Some(signature) = signature else {
            return;
        };
        let supplied = self.bind_arguments(arguments, signature, label, span, kind);
        if !arguments.iter().any(|a| a.spread) {
            self.check_arity(arguments.len(), &supplied, signature, label, span);
        }
    }

    fn evaluate_arguments(
        &mut self,
        arguments: &[Argument],
        signature: Option<&Signature>,
        scope: &mut Scope,
    ) {
        for (position, argument) in arguments.iter().enumerate() {
            let parameter = signature.and_then(|s| {
                if let Some(name) = &argument.name {
                    s.parameters.iter().find(|p| p.name == name.name)
                } else {
                    s.parameters
                        .get(position)
                        .or_else(|| s.parameters.last().filter(|p| p.variadic))
                }
            });
            if parameter.is_some_and(|p| p.by_reference) {
                self.ensure_reference(&argument.value, scope);
            }
            self.expression(&argument.value, scope, signature.is_none());
            if signature.is_none()
                && let ExpressionKind::Variable(Variable::Named(name)) = &argument.value.kind
                && scope.get(name).definition == Definition::Absent
            {
                scope.variables.insert(
                    name.clone(),
                    crate::scope::VariableInfo {
                        definition: Definition::Unknown,
                        ty: ValueType::Mixed,
                    },
                );
            }
        }
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

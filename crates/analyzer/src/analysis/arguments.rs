//! Argument binding: `f(b: 2, a: 1)` supplies parameters by name.
//! Unknown facts stay silent; no PHP code is loaded or executed.

use super::*;
use crate::index::Signature;

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
        for (position, argument) in arguments.iter().enumerate() {
            let parameter = signature.and_then(|s| {
                if let Some(name) = &argument.name {
                    s.parameters.iter().find(|p| p.name == name.name)
                } else {
                    s.parameters.get(position)
                }
            });
            if parameter.is_some_and(|p| p.by_reference) {
                self.ensure_reference(&argument.value, scope);
            }
            if signature.is_none() {
                self.expression(&argument.value, scope, true);
                if let ExpressionKind::Variable(Variable::Named(name)) = &argument.value.kind
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
            } else {
                self.expression(&argument.value, scope, false);
            }
        }
        self.record_exception(scope);
        let Some(signature) = signature else {
            return;
        };
        let variadic = signature.parameters.iter().any(|p| p.variadic);
        let spread = arguments.iter().any(|a| a.spread);
        let mut supplied = std::collections::BTreeSet::new();
        for (position, argument) in arguments.iter().enumerate() {
            if let Some(name) = &argument.name {
                if let Some(i) = signature
                    .parameters
                    .iter()
                    .position(|p| p.name == name.name)
                {
                    supplied.insert(i);
                } else if !variadic {
                    self.report(
                        match kind {
                            CallKind::Direct => {
                                Finding::NamedArgument(label.into(), name.name.clone())
                            }
                            CallKind::Callable => {
                                Finding::KnownNamedArgument(label.into(), name.name.clone())
                            }
                        },
                        span,
                    );
                }
            } else if !argument.spread {
                supplied.insert(position);
            }
        }
        if spread {
            return;
        }
        let missing = signature
            .parameters
            .iter()
            .enumerate()
            .any(|(i, p)| !p.optional && !p.variadic && !supplied.contains(&i));
        if missing
            || (!variadic
                && !signature.accepts_extra
                && arguments.len() > signature.parameters.len())
        {
            let required = signature
                .parameters
                .iter()
                .filter(|p| !p.optional && !p.variadic)
                .count();
            self.report(
                Finding::ArgumentCount(format!(
                    "{label} called with {} arguments; {} required, \
                {} accepted{}.",
                    arguments.len(),
                    required,
                    signature.parameters.len(),
                    if variadic { " or more" } else { "" }
                )),
                span,
            );
        }
    }
}

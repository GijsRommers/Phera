//! A named call such as `strlen("x")` checks the resolved function signature.
//! Within a namespace the unqualified call can fall back to the global function.

use super::*;
use phera_source::deprecations::{ItemKind, omitted};

struct ResolvedCall {
    signature: Option<crate::index::Signature>,
    label: String,
    intrinsic: Option<String>,
    kind: CallKind,
}

impl Analyzer<'_> {
    pub fn call(
        &mut self,
        callee: &Expression,
        arguments: &Arguments,
        scope: &mut Scope,
        span: Span,
    ) -> ValueType {
        let resolved = if let ExpressionKind::Name(name) = &callee.kind
            && callee.span == name.span
        {
            self.named_call(name, scope)
        } else {
            let ty = self.expression(callee, scope, false);
            if ty == ValueType::Never {
                return ty;
            }
            let label = if matches!(ty, ValueType::StringLiteral(_) | ValueType::Shape(_)) {
                format!("callable {ty}")
            } else {
                "callable".into()
            };
            ResolvedCall {
                signature: self.callable_signature(ty),
                label,
                intrinsic: None,
                kind: CallKind::Callable,
            }
        };
        // A class string used to probe availability can name a missing
        // class. The same string outside the probe still needs validation.
        let probe = match (resolved.intrinsic.as_deref(), arguments) {
            (
                Some("class_exists" | "interface_exists" | "trait_exists" | "enum_exists"),
                Arguments::List(items),
            ) => items.first().and_then(|item| self.class_probe(&item.value)),
            _ => None,
        };
        let inserted = probe.filter(|symbol| scope.known.insert(symbol.clone()));
        let facts = self.argument_facts(
            arguments,
            resolved.signature.as_ref(),
            &resolved.label,
            scope,
            callee.span,
            resolved.kind,
        );
        if let Some(symbol) = inserted {
            scope.known.remove(&symbol);
        }
        if facts.evaluation == Evaluation::Never {
            return ValueType::Never;
        }
        if !matches!(arguments, Arguments::FirstClassCallable) {
            if let Some(signature) = &resolved.signature {
                self.unbound_invocation(signature);
            }
            self.apply_global_effects(resolved.signature.as_ref(), scope);
        }
        if let Some(name) = &resolved.intrinsic {
            self.intrinsic(name, arguments, scope);
        }
        if matches!(arguments, Arguments::FirstClassCallable) {
            resolved
                .signature
                .map_or(ValueType::Mixed, crate::index::Signature::as_closure)
        } else {
            self.discarded_function_result(
                span,
                &resolved.label,
                resolved.intrinsic.as_deref(),
                resolved.signature.as_ref(),
            );
            if self.level >= 4
                && let Some(name) = &resolved.intrinsic
                && let Some(ty) =
                    self.constant_type_test(name, arguments, &facts.types, scope, span)
            {
                return ty;
            }
            if self.level >= 5
                && resolved.intrinsic.as_deref() == Some("call_user_func")
                && let Arguments::List(arguments) = arguments
                && let (Some((_, passed)), Some((callback, types))) =
                    (arguments.split_first(), facts.types.split_first())
                && let Some(signature) = self.callable_signature(callback.clone())
            {
                self.check_argument_contracts(
                    passed,
                    types,
                    &signature,
                    "callable passed to call_user_func()",
                    scope,
                );
                return signature.returns;
            }
            if self.level >= 2
                && let Some(name) = &resolved.intrinsic
                && let Some(ty) = self.builtin_result(name, arguments, &facts.types, scope)
            {
                return ty;
            }
            resolved.signature.map_or(ValueType::Mixed, |s| {
                s.infer_return(arguments, &facts.types, self.index)
            })
        }
    }

    fn named_call(&mut self, name: &Name, scope: &Scope) -> ResolvedCall {
        let candidates = self.context.names.resolve(name, UseKind::Function);
        let found = candidates.iter().find_map(|name| {
            self.index
                .functions
                .get(&name.to_ascii_lowercase())
                .map(|signature| (name, signature.clone().typed(self.index)))
        });
        if found.is_none() {
            let excluded = candidates
                .last()
                .and_then(|name| omitted(ItemKind::Function, name));
            if let Some(item) = excluded {
                self.report(Finding::NotSupported(item), name.span);
            } else if !candidates.iter().any(|name| {
                self.context
                    .known
                    .contains(&Symbol::Function(name.to_ascii_lowercase()))
                    || scope
                        .known
                        .contains(&Symbol::Function(name.to_ascii_lowercase()))
            }) {
                self.report(Finding::UnknownFunction(candidates[0].clone()), name.span);
            }
        }
        let intrinsic = found
            .as_ref()
            .filter(|(name, _)| {
                self.index
                    .builtin_functions
                    .contains(&name.to_ascii_lowercase())
            })
            .map(|(name, _)| name.to_ascii_lowercase());
        ResolvedCall {
            label: format!(
                "{}()",
                found.as_ref().map_or(&candidates[0], |(name, _)| name)
            ),
            signature: found.map(|(_, signature)| signature),
            intrinsic,
            kind: CallKind::Direct,
        }
    }
}

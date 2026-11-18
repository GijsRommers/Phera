//! Binding constructs: `[$a, $b] = $pair;`, `global $a;`, and closure captures.
//! Reference writes initialize variables, while ordinary closure uses read them.

use super::*;
use crate::index::{Location, Signature};

impl Analyzer<'_> {
    pub fn ensure_reference(&mut self, expression: &Expression, scope: &mut Scope) {
        if let ExpressionKind::Variable(Variable::Named(name)) = &expression.kind {
            if scope.get(name).definition != Definition::Definite {
                scope.define(name, ValueType::Mixed);
            }
        } else {
            self.write(expression, ValueType::Mixed, scope);
        }
    }

    pub fn external_reference(&self, target: &Expression, scope: &mut Scope) {
        let mut pending = vec![target];
        while let Some(target) = pending.pop() {
            match &target.kind {
                ExpressionKind::Variable(Variable::Named(name)) => {
                    scope.external_references.insert(name.clone());
                }
                ExpressionKind::List(items) => {
                    pending.extend(items.iter().flatten().map(|item| &item.value));
                }
                ExpressionKind::Array(items) => {
                    pending.extend(items.iter().map(|item| &item.value));
                }
                _ => {}
            }
        }
    }

    pub fn parameter_scope(&mut self, parameters: &[Parameter], scope: &mut Scope) {
        for parameter in parameters {
            self.check_type(parameter.ty.as_ref());
            if let Some(default) = &parameter.default {
                self.expression(default, scope, false);
            }
            let ty = if parameter.variadic {
                ValueType::Array
            } else {
                ValueType::parameter(parameter, &self.context.names)
            };
            if parameter.by_reference {
                scope.external_references.insert(parameter.name.clone());
            }
            let ty = self
                .context
                .class
                .as_ref()
                .map_or(ty.clone(), |class| self.relative_type(ty, class));
            scope.define(&parameter.name, ty);
        }
    }

    pub fn closure(&mut self, closure: &Closure, outer: &mut Scope, span: Span) -> ValueType {
        let mut inner = Scope::default();
        for capture in &closure.uses {
            if capture.by_reference {
                if outer.get(&capture.name).definition != Definition::Definite {
                    outer.define(&capture.name, ValueType::Mixed);
                }
            } else {
                self.read(&capture.name, outer, capture.span, false);
            }
            if capture.by_reference {
                outer.external_references.insert(capture.name.clone());
                inner.external_references.insert(capture.name.clone());
            }
            inner.define(
                &capture.name,
                if capture.by_reference {
                    ValueType::Mixed
                } else {
                    outer.get(&capture.name).ty
                },
            );
        }
        let previous = self.context.clone();
        let exceptions = std::mem::take(&mut self.exception_scopes);
        let controls = std::mem::take(&mut self.controls);
        let returns = std::mem::take(&mut self.return_scopes);
        self.context.known.extend(outer.known.iter().cloned());
        self.context.scope_kind = ScopeKind::Function;
        self.context.incomplete = false;
        self.context.object = previous.object && !closure.is_static;
        self.context.unbound_this = (!closure.is_static && !previous.object).then(Vec::new);
        if !closure.is_static {
            self.apply_binding(span);
        }
        self.context.function = "Closure".into();
        self.context.declared_return = closure.return_type.is_some();
        self.context.by_reference = closure.by_reference;
        self.context.returns =
            ValueType::declared(closure.return_type.as_ref(), &self.context.names);
        self.parameter_scope(&closure.parameters, &mut inner);
        self.check_type(closure.return_type.as_ref());
        let flow = self.block(&closure.body, &mut inner);
        self.check_return_path(flow, &closure.body, span);
        let mut signature = Signature::new(
            &closure.parameters,
            closure.return_type.as_ref(),
            &self.context.names,
            Location {
                file: self.file.path.clone(),
                line: self.file.lines.line(span.start),
            },
        );
        signature.unbound_this = self.context.unbound_this.take().unwrap_or_default();
        signature.global_effects = crate::global_effects::of(&closure.body);
        self.context = previous;
        self.exception_scopes = exceptions;
        self.controls = controls;
        self.return_scopes = returns;
        ValueType::Callable(Box::new(signature))
    }

    pub fn arrow(&mut self, arrow: &ArrowFunction, outer: &mut Scope, span: Span) -> ValueType {
        let previous = self.context.clone();
        let exceptions = std::mem::take(&mut self.exception_scopes);
        let controls = std::mem::take(&mut self.controls);
        let returns = std::mem::take(&mut self.return_scopes);
        let mut inner = outer.clone();
        self.context.known.extend(outer.known.iter().cloned());
        self.context.scope_kind = ScopeKind::Function;
        self.context.incomplete = false;
        self.context.object = previous.object && !arrow.is_static;
        self.context.unbound_this = (!arrow.is_static && !previous.object).then(Vec::new);
        if !arrow.is_static {
            self.apply_binding(span);
        }
        self.parameter_scope(&arrow.parameters, &mut inner);
        self.check_type(arrow.return_type.as_ref());
        self.expression(&arrow.body, &mut inner, false);
        if ValueType::declared(arrow.return_type.as_ref(), &self.context.names) == ValueType::Void {
            self.report(Finding::VoidReturn, span);
        }
        let mut signature = Signature::new(
            &arrow.parameters,
            arrow.return_type.as_ref(),
            &self.context.names,
            Location {
                file: self.file.path.clone(),
                line: self.file.lines.line(span.start),
            },
        );
        signature.unbound_this = self.context.unbound_this.take().unwrap_or_default();
        signature.global_effects = crate::global_effects::expression(&arrow.body);
        self.context = previous;
        self.exception_scopes = exceptions;
        self.controls = controls;
        self.return_scopes = returns;
        ValueType::Callable(Box::new(signature))
    }
}

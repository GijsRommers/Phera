//! Binding constructs: `[$a, $b] = $pair;`, `global $a;`, and closure captures.
//! Reference writes initialize variables, while ordinary closure uses read them.

use super::*;
use crate::index::{Location, Signature};

impl Analyzer<'_> {
    pub fn write(&mut self, target: &Expression, ty: ValueType, scope: &mut Scope) {
        let mut pending = vec![(target, ty)];
        while let Some((target, ty)) = pending.pop() {
            match &target.kind {
                ExpressionKind::Variable(Variable::Named(name)) => scope.define(name, ty.clone()),
                ExpressionKind::Variable(Variable::Dynamic(e)) => {
                    self.expression(e, scope, false);
                    scope.unknown_writes();
                }
                ExpressionKind::List(items) => {
                    for item in items.iter().flatten() {
                        if let Some(key) = &item.key {
                            self.expression(key, scope, false);
                        }
                        pending.push((&item.value, ValueType::Mixed));
                    }
                }
                ExpressionKind::Array(items) => {
                    pending.extend(items.iter().map(|i| (&i.value, ValueType::Mixed)))
                }
                ExpressionKind::Index { target, index } => {
                    if let Some(index) = index {
                        self.expression(index, scope, false);
                    }
                    let array_type =
                        if let ExpressionKind::Variable(Variable::Named(name)) = &target.kind {
                            let old = scope.get(name);
                            if old.definition == Definition::Absent {
                                ValueType::Array
                            } else {
                                old.ty
                            }
                        } else {
                            ValueType::Mixed
                        };
                    pending.push((target, array_type));
                }
                ExpressionKind::Property { target, name, .. } => {
                    let ty = self.expression(target, scope, false);
                    self.property(&ty, name, scope, target.span, false, Access::object(target));
                }
                ExpressionKind::StaticProperty { .. } => {
                    self.expression(target, scope, false);
                }
                _ => {
                    self.expression(target, scope, true);
                }
            }
        }
    }

    pub fn ensure_reference(&mut self, expression: &Expression, scope: &mut Scope) {
        if let ExpressionKind::Variable(Variable::Named(name)) = &expression.kind {
            if scope.get(name).definition != Definition::Definite {
                scope.define(name, ValueType::Mixed);
            }
        } else {
            self.write(expression, ValueType::Mixed, scope);
        }
    }

    pub fn parameter_scope(&mut self, parameters: &[Parameter], scope: &mut Scope) {
        for parameter in parameters {
            self.check_type(parameter.ty.as_ref());
            if let Some(default) = &parameter.default {
                self.expression(default, scope, false);
            }
            scope.define(
                &parameter.name,
                ValueType::declared(parameter.ty.as_ref(), &self.context.names),
            );
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
            inner.define(&capture.name, outer.get(&capture.name).ty);
        }
        let previous = self.context.clone();
        let exceptions = std::mem::take(&mut self.exception_scopes);
        let controls = std::mem::take(&mut self.controls);
        let returns = std::mem::take(&mut self.return_scopes);
        self.context.incomplete = false;
        self.context.object = previous.object && !closure.is_static;
        self.context.function = "Closure".into();
        self.context.declared_return = closure.return_type.is_some();
        self.context.by_reference = closure.by_reference;
        self.context.returns =
            ValueType::declared(closure.return_type.as_ref(), &self.context.names);
        self.parameter_scope(&closure.parameters, &mut inner);
        self.check_type(closure.return_type.as_ref());
        let flow = self.block(&closure.body, &mut inner);
        self.check_return_path(flow, &closure.body, span);
        let signature = Signature::new(
            &closure.parameters,
            closure.return_type.as_ref(),
            &self.context.names,
            Location {
                file: self.file.path.clone(),
                line: self.file.lines.line(span.start),
            },
        );
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
        self.context.incomplete = false;
        self.context.object = previous.object && !arrow.is_static;
        self.parameter_scope(&arrow.parameters, &mut inner);
        self.check_type(arrow.return_type.as_ref());
        self.expression(&arrow.body, &mut inner, false);
        if ValueType::declared(arrow.return_type.as_ref(), &self.context.names) == ValueType::Void {
            self.report(Finding::VoidReturn, span);
        }
        let signature = Signature::new(
            &arrow.parameters,
            arrow.return_type.as_ref(),
            &self.context.names,
            Location {
                file: self.file.path.clone(),
                line: self.file.lines.line(span.start),
            },
        );
        self.context = previous;
        self.exception_scopes = exceptions;
        self.controls = controls;
        self.return_scopes = returns;
        ValueType::Callable(Box::new(signature))
    }
}

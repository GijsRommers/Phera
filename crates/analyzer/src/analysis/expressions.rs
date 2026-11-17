//! Expression facts: `new DateTime` has a known class; `$unknown()` stays unknown.
//! The explicit depth bound also protects generated left-deep expression chains.

use super::*;

impl Analyzer<'_> {
    pub fn expression(
        &mut self,
        expression: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ValueType {
        if self.depth >= MAX_DEPTH {
            self.context.incomplete = true;
            scope.dynamic = true;
            return ValueType::Mixed;
        }
        self.depth += 1;
        let ty = self.expression_inner(expression, scope, quiet);
        self.depth -= 1;
        ty
    }

    fn expression_inner(
        &mut self,
        expression: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ValueType {
        match &expression.kind {
            ExpressionKind::Literal(literal) => match literal {
                Literal::Null => ValueType::Null,
                Literal::Bool(_) => ValueType::Bool,
                Literal::Int(_) => ValueType::Int,
                Literal::Float(_) => ValueType::Float,
                Literal::String(_) => ValueType::String,
            },
            ExpressionKind::Variable(Variable::Named(name)) => {
                self.read(name, scope, expression.span, quiet)
            }
            ExpressionKind::Variable(Variable::Dynamic(name)) => {
                self.expression(name, scope, false);
                ValueType::Mixed
            }
            ExpressionKind::Name(name) => {
                self.constant_name(name, scope);
                ValueType::Mixed
            }
            ExpressionKind::MagicConstant(_) => ValueType::String,
            ExpressionKind::Assign { target, value } => {
                let constant = crate::constants::value(value, scope);
                let ty = self.expression(value, scope, false);
                self.write(target, ty.clone(), scope);
                if let ExpressionKind::Variable(Variable::Named(name)) = &target.kind
                    && let Some(value) = constant
                {
                    scope.values.insert(name.clone(), value);
                }
                ty
            }
            ExpressionKind::AssignReference { target, value } => {
                self.ensure_reference(value, scope);
                let ty = self.expression(value, scope, true);
                self.write(target, ty.clone(), scope);
                if let (
                    ExpressionKind::Variable(Variable::Named(left)),
                    ExpressionKind::Variable(Variable::Named(right)),
                ) = (&target.kind, &value.kind)
                {
                    scope.reference(left, right);
                }
                ty
            }
            ExpressionKind::CompoundAssign {
                target,
                value,
                operator,
            } => {
                self.expression(target, scope, *operator == BinaryOperator::NullCoalesce);
                let ty = self.expression(value, scope, false);
                self.write(target, ty.clone(), scope);
                ty
            }
            ExpressionKind::IncDec { target, .. } => {
                self.expression(target, scope, quiet);
                self.write(target, ValueType::Mixed, scope);
                ValueType::Mixed
            }
            ExpressionKind::Call { callee, arguments } => self.call(callee, arguments, scope),
            ExpressionKind::New { class, arguments } => self.new_object(class, arguments, scope),
            ExpressionKind::MethodCall {
                target,
                name,
                arguments,
                ..
            } => {
                let ty = self.expression(target, scope, quiet);
                self.method_call(
                    &ty,
                    name,
                    arguments,
                    scope,
                    expression.span,
                    Access::object(target),
                )
            }
            ExpressionKind::StaticCall {
                class,
                name,
                arguments,
            } => {
                let class_name = self.class_expression(class, scope);
                let ty = class_name.map_or(ValueType::Mixed, ValueType::Class);
                let forwarding = matches!(&class.kind, ExpressionKind::Name(n)
                    if matches!(n.text.to_lowercase().as_str(), "self" | "static" | "parent"))
                    && self.context.object;
                self.method_call(
                    &ty,
                    name,
                    arguments,
                    scope,
                    expression.span,
                    if forwarding {
                        Access::Forwarded
                    } else {
                        Access::Static
                    },
                )
            }
            ExpressionKind::Property { target, name, .. } => {
                let ty = self.expression(target, scope, quiet);
                self.property(
                    &ty,
                    name,
                    scope,
                    expression.span,
                    quiet,
                    Access::object(target),
                )
            }
            ExpressionKind::StaticProperty { class, name } => {
                let class = self.class_expression(class, scope);
                if let Variable::Named(name) = name
                    && let Some(class) = class
                {
                    return self.named_property(
                        &class,
                        name,
                        expression.span,
                        quiet,
                        PropertyAccess::Static,
                    );
                }
                ValueType::Mixed
            }
            ExpressionKind::ClassConstant { class, name } => {
                let class = self.class_expression(class, scope);
                if let (Some(class), MemberName::Identifier(name)) = (class, name)
                    && !name.name.eq_ignore_ascii_case("class")
                    && self.index.class(&class).is_some()
                    && self.index.complete(&class)
                    && !self.index.constant(&class, &name.name)
                {
                    self.report(
                        Finding::UnknownClassConstant(format!("{class}::{}", name.name)),
                        name.span,
                    );
                }
                ValueType::Mixed
            }
            ExpressionKind::Isset(items) => {
                for item in items {
                    self.expression(item, scope, true);
                }
                ValueType::Bool
            }
            ExpressionKind::Empty(item) => {
                self.expression(item, scope, true);
                ValueType::Bool
            }
            ExpressionKind::Binary {
                operator,
                left,
                right,
            } => self.binary(*operator, left, right, scope, quiet),
            ExpressionKind::Ternary {
                condition,
                then,
                otherwise,
            } => self.ternary(condition, then.as_deref(), otherwise, scope),
            ExpressionKind::Closure(closure) => self.closure(closure, scope, expression.span),
            ExpressionKind::ArrowFunction(arrow) => self.arrow(arrow, scope, expression.span),
            ExpressionKind::InstanceOf { value, class } => {
                self.expression(value, scope, quiet);
                self.class_expression(class, scope);
                ValueType::Bool
            }
            ExpressionKind::Throw(value) => {
                self.expression(value, scope, false);
                self.record_exception(scope);
                ValueType::Never
            }
            ExpressionKind::Exit(value) => {
                if let Some(value) = value {
                    self.expression(value, scope, false);
                }
                ValueType::Never
            }
            ExpressionKind::Include { path, .. } | ExpressionKind::Eval(path) => {
                self.expression(path, scope, false);
                self.record_exception(scope);
                scope.dynamic = true;
                ValueType::Mixed
            }
            _ => {
                for child in crate::walk::children(expression) {
                    self.expression(child, scope, quiet);
                }
                match expression.kind {
                    ExpressionKind::Array(_) => ValueType::Array,
                    ExpressionKind::InterpolatedString(_) => ValueType::String,
                    _ => ValueType::Mixed,
                }
            }
        }
    }

    fn constant_name(&mut self, name: &Name, scope: &Scope) {
        if matches!(name.text.to_lowercase().as_str(), "true" | "false" | "null") {
            return;
        }
        let candidates = self.context.names.resolve(name, UseKind::Constant);
        if !candidates.iter().any(|n| {
            self.index.constants.contains_key(n)
                || scope.known.contains(&Symbol::Constant(n.clone()))
        }) {
            if name.text.starts_with("__") && name.text.ends_with("__") {
                self.report(Finding::MagicConstant(name.text.clone()), name.span);
            } else {
                self.report(Finding::UnknownConstant(candidates[0].clone()), name.span);
            }
        }
    }

    fn binary(
        &mut self,
        operator: BinaryOperator,
        left: &Expression,
        right: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ValueType {
        let left_ty = self.expression(
            left,
            scope,
            quiet || operator == BinaryOperator::NullCoalesce,
        );
        if matches!(
            operator,
            BinaryOperator::And | BinaryOperator::Or | BinaryOperator::NullCoalesce
        ) {
            let before = scope.clone();
            self.refine(left, scope, operator != BinaryOperator::Or);
            let right_ty = self.expression(right, scope, quiet);
            *scope = Scope::join(&[before, scope.clone()]);
            return if operator == BinaryOperator::NullCoalesce {
                left_ty.join(&right_ty)
            } else {
                ValueType::Bool
            };
        }
        let right_ty = self.expression(right, scope, quiet);
        if matches!(
            operator,
            BinaryOperator::Divide | BinaryOperator::Modulo | BinaryOperator::Power
        ) {
            self.record_exception(scope);
        }
        match operator {
            BinaryOperator::Concat => ValueType::String,
            BinaryOperator::Equal
            | BinaryOperator::NotEqual
            | BinaryOperator::Identical
            | BinaryOperator::NotIdentical
            | BinaryOperator::Less
            | BinaryOperator::Greater
            | BinaryOperator::LessEqual
            | BinaryOperator::GreaterEqual => ValueType::Bool,
            _ => left_ty.join(&right_ty),
        }
    }

    fn ternary(
        &mut self,
        condition: &Expression,
        then: Option<&Expression>,
        otherwise: &Expression,
        scope: &mut Scope,
    ) -> ValueType {
        let condition_ty = self.expression(condition, scope, false);
        let mut left = scope.clone();
        let mut right = scope.clone();
        self.refine(condition, &mut left, true);
        self.refine(condition, &mut right, false);
        let left_ty = then.map_or(condition_ty, |e| self.expression(e, &mut left, false));
        let right_ty = self.expression(otherwise, &mut right, false);
        *scope = Scope::join(&[left, right]);
        left_ty.join(&right_ty)
    }
}

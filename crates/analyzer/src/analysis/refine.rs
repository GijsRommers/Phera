//! Guard facts: `isset($x) && print($x)` reads a defined variable on the right.
//! A work list handles arbitrarily long boolean chains without Rust recursion.

use super::*;

impl Analyzer<'_> {
    pub fn refine(&mut self, expression: &Expression, scope: &mut Scope, truth: bool) {
        let mut pending = vec![(expression, truth)];
        while let Some((expression, truth)) = pending.pop() {
            match &expression.kind {
                ExpressionKind::Isset(items) if truth => {
                    for item in items {
                        self.refine_defined(item, scope);
                    }
                }
                ExpressionKind::Empty(item) if !truth => self.refine_defined(item, scope),
                ExpressionKind::Unary {
                    operator: UnaryOperator::Not,
                    operand,
                } => pending.push((operand, !truth)),
                ExpressionKind::Binary {
                    operator: BinaryOperator::And,
                    left,
                    right,
                } if truth => {
                    pending.push((left, true));
                    pending.push((right, true));
                }
                ExpressionKind::Binary {
                    operator: BinaryOperator::Or,
                    left,
                    right,
                } if !truth => {
                    pending.push((left, false));
                    pending.push((right, false));
                }
                ExpressionKind::Call {
                    callee,
                    arguments: Arguments::List(arguments),
                } if truth => self.refine_existence(callee, arguments, scope),
                ExpressionKind::InstanceOf { value, class } if truth => {
                    if let ExpressionKind::Variable(Variable::Named(name)) = &value.kind
                        && let ExpressionKind::Name(class) = &class.kind
                        && let Some(class) = self.class_name(class, false)
                    {
                        scope.define(name, ValueType::Class(class));
                    }
                }
                _ => {}
            }
        }
    }

    fn refine_defined(&self, expression: &Expression, scope: &mut Scope) {
        if let ExpressionKind::Variable(Variable::Named(name)) = &expression.kind {
            let ty = scope.get(name).ty;
            scope.define(name, ty);
        }
    }

    fn refine_existence(&self, callee: &Expression, arguments: &[Argument], scope: &mut Scope) {
        if let ExpressionKind::Name(name) = &callee.kind
            && let Some(argument) = arguments.first()
            && let ExpressionKind::Literal(Literal::String(bytes)) = &argument.value.kind
            && let Ok(value) = std::str::from_utf8(bytes)
        {
            let symbol = match name.text.to_lowercase().as_str() {
                "function_exists" => Some(Symbol::Function(value.to_lowercase())),
                "class_exists" | "interface_exists" | "trait_exists" | "enum_exists" => {
                    Some(Symbol::Class(value.to_lowercase()))
                }
                "defined" => Some(Symbol::Constant(value.into())),
                _ => None,
            };
            if let Some(symbol) = symbol {
                scope.known.insert(symbol);
            }
        }
    }

    pub fn condition_truth(&self, expression: &Expression, scope: &Scope) -> Option<bool> {
        if let Some(value) = crate::constants::value(expression, scope) {
            return Some(value.truth());
        }
        let mut expression = expression;
        let mut inverted = false;
        while let ExpressionKind::Unary {
            operator: UnaryOperator::Not,
            operand,
        } = &expression.kind
        {
            inverted = !inverted;
            expression = operand;
        }
        if let ExpressionKind::Call {
            callee,
            arguments: Arguments::List(arguments),
        } = &expression.kind
            && let ExpressionKind::Name(name) = &callee.kind
            && name.text.eq_ignore_ascii_case("function_exists")
            && let Some(argument) = arguments.first()
            && let ExpressionKind::Literal(Literal::String(bytes)) = &argument.value.kind
            && let Ok(name) = std::str::from_utf8(bytes)
            && self.index.builtin_functions.contains(&name.to_lowercase())
        {
            return Some(!inverted);
        }
        None
    }
}

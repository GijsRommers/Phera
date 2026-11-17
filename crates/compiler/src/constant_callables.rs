//! Constants can contain static literal closures: `const F = static function () {};`.

use phera_ast::{Arguments, Expression, ExpressionKind, Literal, MemberName};

/// Validate only the initializer tree; variables inside a closure body are ordinary PHP code.
pub(crate) fn callable_constant_error(expression: &Expression) -> Option<&'static str> {
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        match &expression.kind {
            ExpressionKind::Closure(closure) => {
                if !closure.is_static {
                    return Some("Closures in constant expressions must be static");
                }
                if !closure.uses.is_empty() {
                    return Some("Cannot use(...) variables in constant expression");
                }
            }
            ExpressionKind::ArrowFunction(_) => return Some(INVALID),
            ExpressionKind::Call {
                callee,
                arguments: Arguments::FirstClassCallable,
            } => match &callee.kind {
                ExpressionKind::Name(name) if callee.span == name.span => {}
                ExpressionKind::Literal(Literal::String(_)) => {}
                ExpressionKind::Literal(_) => return Some("Illegal function name"),
                _ => return Some("Cannot use dynamic function name in constant expression"),
            },
            ExpressionKind::StaticCall {
                class,
                name,
                arguments: Arguments::FirstClassCallable,
            } => {
                let ExpressionKind::Name(class) = &class.kind else {
                    return Some("Cannot use dynamic class name in constant expression");
                };
                if class.text == "static" {
                    return Some("\"static\" is not allowed in compile-time constants");
                }
                if !matches!(name, MemberName::Identifier(_)) {
                    return Some("Cannot use dynamic method name in constant expression");
                }
            }
            ExpressionKind::MethodCall {
                arguments: Arguments::FirstClassCallable,
                ..
            } => {
                return Some(INVALID);
            }
            _ => append_children(expression, &mut pending),
        }
    }
    None
}

const INVALID: &str = "Constant expression contains invalid operations";

/// Walk nested arrays and constructor defaults without descending through PHP function bodies.
fn append_children<'a>(expression: &'a Expression, pending: &mut Vec<&'a Expression>) {
    match &expression.kind {
        ExpressionKind::Array(items) => {
            for item in items.iter().rev() {
                pending.push(&item.value);
                if let Some(key) = &item.key {
                    pending.push(key);
                }
            }
        }
        ExpressionKind::Unary { operand, .. } => pending.push(operand),
        ExpressionKind::Binary { left, right, .. } => {
            pending.push(right);
            pending.push(left);
        }
        ExpressionKind::Ternary {
            condition,
            then,
            otherwise,
        } => {
            pending.push(otherwise);
            if let Some(then) = then {
                pending.push(then);
            }
            pending.push(condition);
        }
        ExpressionKind::Index { target, index } => {
            if let Some(index) = index {
                pending.push(index);
            }
            pending.push(target);
        }
        ExpressionKind::New {
            arguments: Arguments::List(arguments),
            ..
        } => {
            pending.extend(arguments.iter().rev().map(|argument| &argument.value));
        }
        _ => {}
    }
}

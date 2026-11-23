//! `match ($x) { 1 + 2 => 'three', 4 => 'four' }` can fetch `$x` once.
//!
//! Zend folds scalar operations before deciding whether all conditions are
//! jump-table keys. This check only evaluates operands known at compile time.

use phera_ast::{
    BinaryOperator as B, Expression, ExpressionKind as E, Literal, UnaryOperator as U,
};
use phera_value::{self as php, CollectDiagnostics, Value};

enum Step<'a> {
    Expression(&'a Expression),
    Binary(B),
    Unary(U),
}

pub(super) fn match_constant(condition: &Expression) -> Option<Value> {
    if let E::Literal(literal) = &condition.kind {
        return Some(scalar(literal));
    }
    if !matches!(
        condition.kind,
        E::Binary { .. } | E::Unary { .. } | E::Array(_)
    ) {
        return None;
    }
    let mut work = vec![Step::Expression(condition)];
    let mut results = Vec::new();
    let mut diagnostics = CollectDiagnostics::default();
    while let Some(step) = work.pop() {
        let value = match step {
            Step::Expression(expression) => match &expression.kind {
                E::Literal(literal) => scalar(literal),
                E::Array(items) if items.is_empty() => Value::from(php::PhpArray::new()),
                E::Binary {
                    operator,
                    left,
                    right,
                } => {
                    work.push(Step::Binary(*operator));
                    work.push(Step::Expression(right));
                    work.push(Step::Expression(left));
                    continue;
                }
                E::Unary { operator, operand } => {
                    work.push(Step::Unary(*operator));
                    work.push(Step::Expression(operand));
                    continue;
                }
                _ => return None,
            },
            Step::Binary(operator) => {
                let right = results.pop()?;
                let left = results.pop()?;
                constant_binary(operator, &left, &right, &mut diagnostics)?
            }
            Step::Unary(operator) => {
                let operand = results.pop()?;
                match operator {
                    U::Plus => php::unary_plus(&operand, &mut diagnostics).ok()?,
                    U::Minus => php::negate(&operand, &mut diagnostics).ok()?,
                    U::Not if !operand.is_float() => Value::Bool(!php::to_bool(&operand)),
                    U::BitwiseNot if !operand.is_null() && !operand.is_bool() => {
                        php::bitwise_not(&operand, &mut diagnostics).ok()?
                    }
                    _ => return None,
                }
            }
        };
        if !diagnostics.entries.is_empty() {
            return None;
        }
        results.push(value);
    }
    results.pop()
}

fn scalar(literal: &Literal) -> Value {
    match literal {
        Literal::Null => Value::Null,
        Literal::Bool(value) => Value::Bool(*value),
        Literal::Int(value) => Value::Int(*value),
        Literal::Float(value) => Value::Float(*value),
        Literal::String(bytes) => Value::from(bytes.clone()),
    }
}

fn constant_binary(
    operator: B,
    left: &Value,
    right: &Value,
    diagnostics: &mut CollectDiagnostics,
) -> Option<Value> {
    match operator {
        B::Add => php::add(left, right, diagnostics),
        B::Subtract => php::sub(left, right, diagnostics),
        B::Multiply => php::mul(left, right, diagnostics),
        B::Divide => php::div(left, right, diagnostics),
        B::Modulo => php::modulo(left, right, diagnostics),
        B::Power => php::pow(left, right, diagnostics),
        B::Concat => php::concat(left, right, diagnostics),
        B::BitwiseAnd => php::bitwise_and(left, right, diagnostics),
        B::BitwiseOr => php::bitwise_or(left, right, diagnostics),
        B::BitwiseXor => php::bitwise_xor(left, right, diagnostics),
        B::ShiftLeft => php::shift_left(left, right, diagnostics),
        B::ShiftRight => php::shift_right(left, right, diagnostics),
        B::Equal => php::loose_equals(left, right, diagnostics),
        B::Identical => php::strict_equals(left, right, diagnostics),
        B::NotEqual => php::loose_equals(left, right, diagnostics)
            .map(|equal| Value::Bool(!php::to_bool(&equal))),
        B::NotIdentical => php::strict_equals(left, right, diagnostics)
            .map(|equal| Value::Bool(!php::to_bool(&equal))),
        B::Less => php::less(left, right, diagnostics),
        B::LessEqual => php::less_equal(left, right, diagnostics),
        B::Greater => php::less(right, left, diagnostics),
        B::GreaterEqual => php::less_equal(right, left, diagnostics),
        B::Spaceship => php::compare_op(left, right, diagnostics),
        B::And => Ok(Value::Bool(php::to_bool(left) && php::to_bool(right))),
        B::Or => Ok(Value::Bool(php::to_bool(left) || php::to_bool(right))),
        B::Xor => Ok(Value::Bool(php::to_bool(left) ^ php::to_bool(right))),
        B::NullCoalesce => Ok(if left.is_null() { right } else { left }.clone()),
        B::Pipe => return None,
    }
    .ok()
}

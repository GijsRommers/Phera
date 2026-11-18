//! Small, certain constant facts: `for ($i = 0; $i < 2; ...)` enters its body.
//! Unsupported operations return unknown; this never evaluates user PHP code.

use crate::scope::Scope;
use phera_ast::{BinaryOperator, Expression, ExpressionKind, Literal, UnaryOperator, Variable};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstantValue {
    Int(i64),
    Bool(bool),
    Null,
}

impl ConstantValue {
    pub fn truth(&self) -> bool {
        match self {
            Self::Int(n) => *n != 0,
            Self::Bool(b) => *b,
            Self::Null => false,
        }
    }
}

pub fn value(expression: &Expression, scope: &Scope) -> Option<ConstantValue> {
    evaluate(expression, scope, 0)
}

fn evaluate(expression: &Expression, scope: &Scope, depth: usize) -> Option<ConstantValue> {
    if depth >= 64 {
        return None;
    }
    match &expression.kind {
        ExpressionKind::Literal(Literal::Int(n)) => Some(ConstantValue::Int(*n)),
        ExpressionKind::Literal(Literal::Bool(b)) => Some(ConstantValue::Bool(*b)),
        ExpressionKind::Literal(Literal::Null) => Some(ConstantValue::Null),
        ExpressionKind::Variable(Variable::Named(name)) => scope.values.get(name).cloned(),
        ExpressionKind::Unary {
            operator: UnaryOperator::Not,
            operand,
        } => Some(ConstantValue::Bool(
            !evaluate(operand, scope, depth + 1)?.truth(),
        )),
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => {
            let left = evaluate(left, scope, depth + 1)?;
            let right = evaluate(right, scope, depth + 1)?;
            match (operator, left, right) {
                (BinaryOperator::Less, ConstantValue::Int(a), ConstantValue::Int(b)) => {
                    Some(ConstantValue::Bool(a < b))
                }
                (BinaryOperator::LessEqual, ConstantValue::Int(a), ConstantValue::Int(b)) => {
                    Some(ConstantValue::Bool(a <= b))
                }
                (BinaryOperator::Greater, ConstantValue::Int(a), ConstantValue::Int(b)) => {
                    Some(ConstantValue::Bool(a > b))
                }
                (BinaryOperator::GreaterEqual, ConstantValue::Int(a), ConstantValue::Int(b)) => {
                    Some(ConstantValue::Bool(a >= b))
                }
                (BinaryOperator::Identical, a, b) => Some(ConstantValue::Bool(a == b)),
                (BinaryOperator::NotIdentical, a, b) => Some(ConstantValue::Bool(a != b)),
                _ => None,
            }
        }
        _ => None,
    }
}

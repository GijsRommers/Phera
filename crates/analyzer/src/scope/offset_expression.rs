//! A guard on `$items[$i - (2 - 1)]` can refine the same offset on a later read.
//! Keep the structure and variable dependencies of small arithmetic keys.

use phera_ast::{BinaryOperator, Expression, ExpressionKind, Literal, Variable};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum OffsetToken {
    Integer(i64),
    Variable(String),
    Add,
    Subtract,
    Multiply,
}

pub(super) fn arithmetic_offset(expression: &Expression) -> Option<Vec<OffsetToken>> {
    let mut tokens = Vec::new();
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        if tokens.len() == 16 {
            return None;
        }
        tokens.push(match &expression.kind {
            ExpressionKind::Literal(Literal::Int(value)) => OffsetToken::Integer(*value),
            ExpressionKind::Variable(Variable::Named(name)) => OffsetToken::Variable(name.clone()),
            ExpressionKind::Binary {
                operator,
                left,
                right,
            } => {
                pending.extend([right.as_ref(), left.as_ref()]);
                match operator {
                    BinaryOperator::Add => OffsetToken::Add,
                    BinaryOperator::Subtract => OffsetToken::Subtract,
                    BinaryOperator::Multiply => OffsetToken::Multiply,
                    _ => return None,
                }
            }
            _ => return None,
        });
    }
    Some(tokens)
}

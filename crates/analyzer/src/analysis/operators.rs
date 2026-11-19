//! `$counter += 1` stores the sum, and `$text .= 1` stores a string.
//! A numeric operation may promote an overflowing integer to a float.

use super::*;

pub fn binary_type(operator: BinaryOperator, left: &ValueType, right: &ValueType) -> ValueType {
    use BinaryOperator as Op;
    use ValueType as T;
    match operator {
        Op::Concat => T::String,
        Op::And
        | Op::Or
        | Op::Xor
        | Op::Equal
        | Op::NotEqual
        | Op::Identical
        | Op::NotIdentical
        | Op::Less
        | Op::LessEqual
        | Op::Greater
        | Op::GreaterEqual => T::Bool,
        Op::Spaceship => T::IntRange(-1, 1),
        Op::ShiftLeft | Op::ShiftRight | Op::Modulo => T::Int,
        Op::BitwiseAnd | Op::BitwiseOr | Op::BitwiseXor => {
            if is_string(left) && is_string(right) {
                T::String
            } else {
                T::Int.join(&T::String)
            }
        }
        Op::Add if is_array(left) && is_array(right) => T::Array,
        Op::Add | Op::Subtract | Op::Multiply | Op::Power | Op::Divide => {
            if let (T::IntLiteral(left), T::IntLiteral(right)) = (left, right) {
                let literal = match operator {
                    Op::Add => left.checked_add(*right),
                    Op::Subtract => left.checked_sub(*right),
                    Op::Multiply => left.checked_mul(*right),
                    _ => None,
                };
                if let Some(literal) = literal {
                    return T::IntLiteral(literal);
                }
            }
            if is_numeric(left) && is_numeric(right) {
                if left == &T::Float || right == &T::Float {
                    T::Float
                } else {
                    T::Int.join(&T::Float)
                }
            } else {
                T::Mixed
            }
        }
        Op::NullCoalesce => left.without_null().join(right),
        Op::Pipe => T::Mixed,
    }
}

pub fn unary_type(operator: UnaryOperator, value: ValueType) -> ValueType {
    use ValueType as T;
    match operator {
        UnaryOperator::ErrorSuppress => value,
        UnaryOperator::Not => match value {
            T::BoolLiteral(value) => T::BoolLiteral(!value),
            _ => T::Bool,
        },
        UnaryOperator::Plus if is_numeric(&value) => value,
        UnaryOperator::Minus if is_numeric(&value) => match value {
            T::IntLiteral(value) => value.checked_neg().map_or(T::Float, T::IntLiteral),
            T::Float => T::Float,
            _ => T::Int.join(&T::Float),
        },
        UnaryOperator::BitwiseNot if is_string(&value) => T::String,
        UnaryOperator::BitwiseNot if is_numeric(&value) => T::Int,
        _ => T::Mixed,
    }
}

fn is_numeric(ty: &ValueType) -> bool {
    matches!(
        ty,
        ValueType::Int | ValueType::IntLiteral(_) | ValueType::IntRange(..) | ValueType::Float
    )
}

fn is_string(ty: &ValueType) -> bool {
    matches!(
        ty,
        ValueType::String
            | ValueType::StringLiteral(_)
            | ValueType::StringRefinement(_)
            | ValueType::ClassString(_)
    )
}

fn is_array(ty: &ValueType) -> bool {
    match ty {
        ValueType::Array => true,
        ValueType::Collection(collection) => {
            collection.kind != crate::types::CollectionKind::Iterable
        }
        ValueType::Shape(shape) => shape.kind != crate::types::ShapeKind::Object,
        _ => false,
    }
}

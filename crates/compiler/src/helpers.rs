//! Literal recognition and AST operator/type conversion helpers.
use crate::ir::{Binary, Constant};
use phera_ast as ast;
pub(crate) fn constant(error: &ast::Expression) -> Option<Constant> {
    match &error.kind {
        ast::ExpressionKind::Array(items) if items.is_empty() => Some(Constant::EmptyArray),
        ast::ExpressionKind::Unary {
            operator: ast::UnaryOperator::Minus,
            operand,
        } => match constant(operand)? {
            Constant::Int(v) => v
                .checked_neg()
                .map(Constant::Int)
                .or(Some(Constant::Float(-(v as f64)))),
            Constant::Float(v) => Some(Constant::Float(-v)),
            _ => None,
        },
        ast::ExpressionKind::Literal(l) => Some(match l {
            ast::Literal::Null => Constant::Null,
            ast::Literal::Bool(v) => Constant::Bool(*v),
            ast::Literal::Int(v) => Constant::Int(*v),
            ast::Literal::Float(v) => Constant::Float(*v),
            ast::Literal::String(v) => Constant::Bytes(v.clone()),
        }),
        _ => None,
    }
}
pub(crate) fn type_name(t: &ast::Type) -> String {
    match &t.kind {
        ast::TypeKind::Named(index) => index.text.clone(),
        ast::TypeKind::Nullable(t) => format!("?{}", type_name(t)),
        ast::TypeKind::Union(ts) => ts.iter().map(type_name).collect::<Vec<_>>().join("|"),
        ast::TypeKind::Intersection(ts) => ts.iter().map(type_name).collect::<Vec<_>>().join("&"),
    }
}
pub(crate) fn binary(instruction: ast::BinaryOperator) -> Option<Binary> {
    use ast::BinaryOperator as A;
    Some(match instruction {
        A::Add => Binary::Add,
        A::Subtract => Binary::Subtract,
        A::Multiply => Binary::Multiply,
        A::Divide => Binary::Divide,
        A::Modulo => Binary::Modulo,
        A::Power => Binary::Power,
        A::Concat => Binary::Concat,
        A::BitwiseAnd => Binary::BitwiseAnd,
        A::BitwiseOr => Binary::BitwiseOr,
        A::BitwiseXor => Binary::BitwiseXor,
        A::ShiftLeft => Binary::ShiftLeft,
        A::ShiftRight => Binary::ShiftRight,
        A::Xor => Binary::Xor,
        A::Equal => Binary::Equal,
        A::NotEqual => Binary::NotEqual,
        A::Identical => Binary::Identical,
        A::NotIdentical => Binary::NotIdentical,
        A::Less => Binary::Less,
        A::LessEqual => Binary::LessEqual,
        A::Greater => Binary::Greater,
        A::GreaterEqual => Binary::GreaterEqual,
        A::Spaceship => Binary::Spaceship,
        _ => return None,
    })
}
pub(crate) fn writable(error: &ast::Expression) -> bool {
    let mut node = error;
    while let ast::ExpressionKind::Index { target, .. } = &node.kind {
        node = target;
    }
    matches!(node.kind, ast::ExpressionKind::Variable(_))
}

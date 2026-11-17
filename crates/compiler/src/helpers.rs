//! Literal recognition and AST operator/type conversion helpers.

use phera_ast as ast;

use crate::ir::{Binary, Constant};
/// Recognize literal values without evaluating arbitrary PHP expressions.
pub(crate) fn constant(expression: &ast::Expression) -> Option<Constant> {
    match &expression.kind {
        ast::ExpressionKind::Array(items) if items.is_empty() => Some(Constant::EmptyArray),
        ast::ExpressionKind::Unary {
            operator: ast::UnaryOperator::Minus,
            operand,
        } => match constant(operand)? {
            Constant::Int(value) => value
                .checked_neg()
                .map(Constant::Int)
                .or(Some(Constant::Float(-(value as f64)))),
            Constant::Float(value) => Some(Constant::Float(-value)),
            _ => None,
        },
        ast::ExpressionKind::Literal(literal) => Some(match literal {
            ast::Literal::Null => Constant::Null,
            ast::Literal::Bool(value) => Constant::Bool(*value),
            ast::Literal::Int(value) => Constant::Int(*value),
            ast::Literal::Float(value) => Constant::Float(*value),
            ast::Literal::String(value) => Constant::Bytes(value.clone()),
        }),
        _ => None,
    }
}

/// Preserve the written type shape for final bytecode type-table emission.
pub(crate) fn type_name(declared_type: &ast::Type) -> String {
    match &declared_type.kind {
        ast::TypeKind::Named(index) => index.text.clone(),
        ast::TypeKind::Nullable(declared_type) => format!("?{}", type_name(declared_type)),
        ast::TypeKind::Union(types) => types.iter().map(type_name).collect::<Vec<_>>().join("|"),
        ast::TypeKind::Intersection(types) => {
            types.iter().map(type_name).collect::<Vec<_>>().join("&")
        }
    }
}

/// Translate supported binary operators without folding or reordering operands.
pub(crate) fn binary(instruction: ast::BinaryOperator) -> Option<Binary> {
    use ast::BinaryOperator;
    Some(match instruction {
        BinaryOperator::Add => Binary::Add,
        BinaryOperator::Subtract => Binary::Subtract,
        BinaryOperator::Multiply => Binary::Multiply,
        BinaryOperator::Divide => Binary::Divide,
        BinaryOperator::Modulo => Binary::Modulo,
        BinaryOperator::Power => Binary::Power,
        BinaryOperator::Concat => Binary::Concat,
        BinaryOperator::BitwiseAnd => Binary::BitwiseAnd,
        BinaryOperator::BitwiseOr => Binary::BitwiseOr,
        BinaryOperator::BitwiseXor => Binary::BitwiseXor,
        BinaryOperator::ShiftLeft => Binary::ShiftLeft,
        BinaryOperator::ShiftRight => Binary::ShiftRight,
        BinaryOperator::Xor => Binary::Xor,
        BinaryOperator::Equal => Binary::Equal,
        BinaryOperator::NotEqual => Binary::NotEqual,
        BinaryOperator::Identical => Binary::Identical,
        BinaryOperator::NotIdentical => Binary::NotIdentical,
        BinaryOperator::Less => Binary::Less,
        BinaryOperator::LessEqual => Binary::LessEqual,
        BinaryOperator::Greater => Binary::Greater,
        BinaryOperator::GreaterEqual => Binary::GreaterEqual,
        BinaryOperator::Spaceship => Binary::Spaceship,
        _ => return None,
    })
}

/// Recognize variable-rooted paths iteratively, including arbitrarily long index chains.
pub(crate) fn writable(expression: &ast::Expression) -> bool {
    if crate::expressions::nullsafe::contains_nullsafe(expression) {
        return false;
    }
    let mut node = expression;
    let mut property = false;
    loop {
        match &node.kind {
            ast::ExpressionKind::Index { target, .. } => node = target,
            ast::ExpressionKind::Property {
                target,
                nullsafe: false,
                ..
            } => {
                property = true;
                node = target;
            }
            _ => break,
        }
    }
    if property || matches!(node.kind, ast::ExpressionKind::StaticProperty { .. }) {
        return true;
    }
    matches!(&node.kind, ast::ExpressionKind::Variable(ast::Variable::Named(name)) if name != "this")
        || matches!(
            &node.kind,
            ast::ExpressionKind::Variable(ast::Variable::Dynamic(_))
        )
}

impl crate::Compiler<'_> {
    /// Class type identifiers obey use aliases and namespaces, unlike builtin types.
    pub(crate) fn declaration_type(&self, declared_type: &ast::Type) -> String {
        match &declared_type.kind {
            ast::TypeKind::Named(name) => {
                let lower = name.text.to_ascii_lowercase();
                match lower.as_str() {
                    "int" | "float" | "string" | "bool" | "array" | "object" | "iterable"
                    | "callable" | "mixed" | "void" | "never" | "null" | "true" | "false"
                    | "self" | "parent" | "static" => lower,
                    _ => self.resolve(name, crate::policies::NameKind::Class).0,
                }
            }
            ast::TypeKind::Nullable(inner) => format!("?{}", self.declaration_type(inner)),
            ast::TypeKind::Union(types) => types
                .iter()
                .map(|member| self.declaration_type(member))
                .collect::<Vec<_>>()
                .join("|"),
            ast::TypeKind::Intersection(types) => types
                .iter()
                .map(|member| self.declaration_type(member))
                .collect::<Vec<_>>()
                .join("&"),
        }
    }
}

/// PHP displays scalar defaults literally and compound constant expressions symbolically.
pub(crate) fn default_display(expression: &ast::Expression) -> String {
    match &expression.kind {
        ast::ExpressionKind::Literal(ast::Literal::Null) => "null".into(),
        ast::ExpressionKind::Literal(ast::Literal::Bool(value)) => value.to_string(),
        ast::ExpressionKind::Literal(ast::Literal::Int(value)) => value.to_string(),
        ast::ExpressionKind::Literal(ast::Literal::Float(value)) => value.to_string(),
        ast::ExpressionKind::Literal(ast::Literal::String(value)) => format!(
            "'{}'",
            String::from_utf8_lossy(value)
                .replace('\\', "\\\\")
                .replace('\'', "\\'")
        ),
        ast::ExpressionKind::Array(items) if items.is_empty() => "[]".into(),
        ast::ExpressionKind::Name(name) => name.text.clone(),
        _ => "<expression>".into(),
    }
}

/// PHP allows an alias of $this as an argument or return source; assigning that alias
/// detaches it instead of replacing the method receiver.
pub(crate) fn reference_capable(expression: &ast::Expression) -> bool {
    writable(expression)
        || matches!(&expression.kind,
        ast::ExpressionKind::Variable(ast::Variable::Named(name)) if name == "this")
}

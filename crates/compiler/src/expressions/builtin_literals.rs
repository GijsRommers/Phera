//! Literal operands for special calls such as `ord('a')` and `defined(23)`.

use phera_ast::{Expression, ExpressionKind, Literal};

use crate::ir::Constant;

pub(super) fn folded_scalar(value: phera_bytecode::Constant) -> Option<Constant> {
    use phera_bytecode::Constant as LiteralValue;
    Some(match value {
        LiteralValue::Null => Constant::Null,
        LiteralValue::Bool(value) => Constant::Bool(value),
        LiteralValue::Int(value) => Constant::Int(value),
        LiteralValue::Float(value) => Constant::Float(value),
        LiteralValue::String(value) => Constant::Bytes(value),
        LiteralValue::Array(_) => return None,
    })
}

/// Zend stringifies literal numeric names before checking `defined(23)`.
pub(super) fn defined_literal(argument: &Expression) -> Option<Vec<u8>> {
    let name = match &argument.kind {
        ExpressionKind::Literal(Literal::String(value)) => value.clone(),
        ExpressionKind::Literal(Literal::Int(value)) => value.to_string().into_bytes(),
        ExpressionKind::Literal(Literal::Float(value)) => phera_value::to_php_string(
            &phera_value::Value::Float(*value),
            &mut phera_value::NoDiagnostics,
        )
        .ok()?
        .as_bytes()
        .to_vec(),
        _ => return None,
    };
    (!name.iter().any(|byte| matches!(byte, b'\\' | b':'))).then_some(name)
}

/// Zend folds strlen on strings, ord on single bytes, and chr on byte values.
/// Values that can raise a runtime diagnostic retain the original instruction.
pub(super) fn fold_builtin(name: &str, argument: &Expression) -> Option<Constant> {
    match (name, &argument.kind) {
        ("strlen", ExpressionKind::Literal(Literal::String(bytes))) => {
            Some(Constant::Int(bytes.len() as i64))
        }
        ("ord", ExpressionKind::Literal(Literal::String(bytes))) => {
            let [byte] = bytes.as_slice() else {
                return None;
            };
            Some(Constant::Int(i64::from(*byte)))
        }
        ("chr", ExpressionKind::Literal(Literal::Int(byte))) => {
            Some(Constant::Bytes(vec![u8::try_from(*byte).ok()?]))
        }
        ("count" | "sizeof", ExpressionKind::Array(items))
            if items.iter().all(|item| {
                item.key.is_none()
                    && !item.spread
                    && !item.by_reference
                    && matches!(item.value.kind, ExpressionKind::Literal(_))
            }) =>
        {
            Some(Constant::Int(items.len() as i64))
        }
        _ => None,
    }
}

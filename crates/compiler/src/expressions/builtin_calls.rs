//! Zend's direct builtin forms, such as `strlen($word)` and `ord('a')`.
//!
//! `namespace App; strlen($word)` remains a normal call because
//! `App\strlen` may exist when that site first executes.

use phera_ast::{Arguments, CastKind, Expression, ExpressionKind, Literal, Variable};
use phera_bytecode::{BuiltinUnary, FetchMode, Reg, TypeMask};

use crate::ir::{Constant, Instruction, PlaceScope};
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Use special forms only for a positional call to an enabled, resolved builtin.
    pub(super) fn compile_builtin_call(
        &mut self,
        name: &str,
        arguments: &Arguments,
        line: usize,
    ) -> Result<Option<Reg>, CompileError> {
        let Arguments::List(arguments) = arguments else {
            return Ok(None);
        };
        let [argument] = arguments.as_slice() else {
            return Ok(None);
        };
        if argument.spread || argument.name.is_some() {
            return Ok(None);
        }
        let Some(enabled) = self.native_functions else {
            return Ok(None);
        };
        let name = name.to_ascii_lowercase();
        if enabled(&name).is_none() {
            return Ok(None);
        }
        let argument = &argument.value;
        if let Some(value) = fold_builtin(&name, argument) {
            return self.literal(value, line).map(Some);
        }
        let cast = match name.as_str() {
            "intval" => Some(CastKind::Int),
            "floatval" | "doubleval" => Some(CastKind::Float),
            "boolval" => Some(CastKind::Bool),
            "strval" => Some(CastKind::String),
            _ => None,
        };
        if let Some(cast) = cast {
            return self.compile_cast(&cast, argument, line).map(Some);
        }
        let Some(operator) = builtin_unary(&name) else {
            return Ok(None);
        };
        let source = self.builtin_operand(argument, line)?;
        let destination = self.allocate_register()?;
        self.emit(
            Instruction::BuiltinUnary {
                destination,
                operator,
                source,
            },
            line,
        );
        Ok(Some(destination))
    }

    /// A unary builtin can read a named local without copying it into a temporary.
    fn builtin_operand(&mut self, argument: &Expression, line: usize) -> Result<Reg, CompileError> {
        if let ExpressionKind::Variable(Variable::Named(_)) = &argument.kind {
            let place = self.readable_place(argument)?;
            if place.scope == PlaceScope::Local {
                return Ok(place.base);
            }
            let destination = self.allocate_register()?;
            self.emit(
                Instruction::Read {
                    destination,
                    place,
                    mode: FetchMode::Read,
                },
                line,
            );
            return Ok(destination);
        }
        self.compile_expression(argument)
    }
}

fn builtin_unary(name: &str) -> Option<BuiltinUnary> {
    use BuiltinUnary as B;
    use TypeMask as T;
    Some(match name {
        "strlen" => B::Strlen,
        "count" => B::Count,
        "sizeof" => B::Sizeof,
        "gettype" => B::GetType,
        "is_resource" => B::IsResource,
        "is_null" => B::TypeCheck(T::NULL),
        "is_bool" => B::TypeCheck(T::BOOL),
        "is_int" | "is_integer" | "is_long" => B::TypeCheck(T::INT),
        "is_float" | "is_double" => B::TypeCheck(T::FLOAT),
        "is_string" => B::TypeCheck(T::STRING),
        "is_array" => B::TypeCheck(T::ARRAY),
        "is_object" => B::TypeCheck(T::OBJECT),
        "is_scalar" => B::TypeCheck(T::BOOL.union(T::INT).union(T::FLOAT).union(T::STRING)),
        _ => return None,
    })
}

/// Zend folds strlen on strings, ord on single bytes, and chr on byte values.
/// Values that can raise a runtime diagnostic retain the original instruction.
fn fold_builtin(name: &str, argument: &Expression) -> Option<Constant> {
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

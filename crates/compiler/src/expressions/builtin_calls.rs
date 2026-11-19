//! Zend's direct builtin forms, such as `strlen($word)` and `ord('a')`.
//!
//! `namespace App; strlen($word)` remains a normal call because
//! `App\strlen` may exist when that site first executes.

use phera_ast::{Arguments, CastKind, Expression, ExpressionKind, Literal, Variable};
use phera_bytecode::{BuiltinFrame, BuiltinUnary, CallbackArguments, FetchMode, Reg, TypeMask};

use crate::ir::{Constant, Instruction, PlaceScope};
use crate::{CompileError, Compiler};

use super::builtin_literals::{defined_literal, fold_builtin, folded_scalar};
use super::builtin_search::literal_search;
use crate::policies::NameKind;

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
        if arguments
            .iter()
            .any(|argument| argument.spread || argument.name.is_some())
        {
            return Ok(None);
        }
        let Some(enabled) = self.native_functions else {
            return Ok(None);
        };
        let name = name.to_ascii_lowercase();
        if (enabled.lookup)(&name).is_none() {
            return Ok(None);
        }
        if name == "sprintf" {
            return self.compile_literal_sprintf(arguments, line);
        }
        let callback_arguments = match (name.as_str(), arguments.len()) {
            ("call_user_func", 1..) => Some(CallbackArguments::Positional),
            ("call_user_func_array", 2) => Some(CallbackArguments::Array),
            _ => None,
        };
        if let Some(kind) = callback_arguments
            && let [callback, arguments @ ..] = arguments.as_slice()
        {
            let callback_value = self.builtin_operand(&callback.value, line)?;
            self.emit(
                Instruction::InitUserCallback {
                    callback: callback_value,
                    arguments: kind,
                },
                line,
            );
            let mut temporary_objects = Vec::new();
            if matches!(callback.value.kind, ExpressionKind::New { .. }) {
                temporary_objects.push(callback_value);
            }
            for argument in arguments {
                let source = self.builtin_operand(&argument.value, line)?;
                self.emit(
                    Instruction::SendUserCallback {
                        source,
                        arguments: kind,
                    },
                    line,
                );
                if matches!(argument.value.kind, ExpressionKind::New { .. }) {
                    temporary_objects.push(source);
                }
            }
            return self.finish_call(temporary_objects, line).map(Some);
        }
        if let Some(operator) = self.frame_builtin(&name, arguments, enabled) {
            let destination = self.allocate_register()?;
            self.emit(
                Instruction::BuiltinFrame {
                    destination,
                    operator,
                },
                line,
            );
            return Ok(Some(destination));
        }
        if let ("array_key_exists", [key, array]) = (name.as_str(), arguments.as_slice()) {
            let key = self.builtin_operand(&key.value, line)?;
            let array = self.builtin_operand(&array.value, line)?;
            let destination = self.allocate_register()?;
            self.emit(
                Instruction::ArrayKeyExists {
                    destination,
                    key,
                    array,
                },
                line,
            );
            return Ok(Some(destination));
        }
        if name == "in_array"
            && let Some(search) = literal_search(arguments)
        {
            if let Some(found) = search.folded() {
                return self.literal(Constant::Bool(found), line).map(Some);
            }
            let needle = self.builtin_operand(search.needle, line)?;
            let destination = self.allocate_register()?;
            self.emit(
                Instruction::InLiteralArray {
                    destination,
                    needle,
                    haystack: search.haystack,
                    strict: search.strict,
                },
                line,
            );
            return Ok(Some(destination));
        }
        let [argument] = arguments.as_slice() else {
            return Ok(None);
        };
        let argument = &argument.value;
        if let ExpressionKind::Literal(Literal::String(bytes)) = &argument.kind
            && let Some(value) = (enabled.fold_string_call)(&name, bytes)
            && let Some(value) = folded_scalar(value)
        {
            return self.literal(value, line).map(Some);
        }
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
        let source = if operator == BuiltinUnary::Defined {
            let Some(name) = defined_literal(argument) else {
                return Ok(None);
            };
            self.literal(name, line)?
        } else {
            self.builtin_operand(argument, line)?
        };
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

    fn frame_builtin(
        &self,
        name: &str,
        arguments: &[phera_ast::Argument],
        enabled: &crate::NativeFunctions<'_>,
    ) -> Option<BuiltinFrame> {
        let function = self.function.name != "{main}";
        match (name, arguments) {
            ("get_called_class", []) => Some(BuiltinFrame::CalledClass),
            ("func_num_args", []) if function => Some(BuiltinFrame::ArgumentCount),
            ("func_get_args", []) if function => Some(BuiltinFrame::ArgumentsFrom(0)),
            ("array_slice", [source, offset]) if function => {
                let ExpressionKind::Literal(Literal::Int(offset)) = &offset.value.kind else {
                    return None;
                };
                let ExpressionKind::Call {
                    callee,
                    arguments: Arguments::List(arguments),
                } = &source.value.kind
                else {
                    return None;
                };
                let ExpressionKind::Name(callee_name) = &callee.kind else {
                    return None;
                };
                let (resolved, fallback) = self.resolve(callee_name, NameKind::Function);
                if *offset < 0
                    || callee.span != callee_name.span
                    || !arguments.is_empty()
                    || fallback.is_some()
                    || !resolved.eq_ignore_ascii_case("func_get_args")
                    || (enabled.lookup)("func_get_args").is_none()
                {
                    return None;
                }
                Some(BuiltinFrame::ArgumentsFrom(
                    u32::try_from(*offset).unwrap_or(u32::MAX),
                ))
            }
            _ => None,
        }
    }

    /// A unary builtin can read a named local without copying it into a temporary.
    pub(super) fn builtin_operand(
        &mut self,
        argument: &Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
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
        "get_class" => B::GetClass,
        "defined" => B::Defined,
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

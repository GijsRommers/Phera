//! Reject invalid parameter defaults before any PHP code executes.

use phera_ast::{Expression, ExpressionKind, Parameter};

use crate::{CompileError, Compiler, constant, ir::Constant, type_name};

impl Compiler<'_> {
    /// PHP checks literal default types at declaration time even if the function is never called.
    pub(crate) fn validate_parameter_default(
        &self,
        parameter: &Parameter,
        default: &Expression,
        line: usize,
    ) -> Result<(), CompileError> {
        if let Some(message) = crate::constant_callables::callable_constant_error(default) {
            return Err(self.error(message, line));
        }
        if let Some(message) = initializer_error(default, true) {
            return Err(self.error(message, line));
        }
        let Some(declaration) = &parameter.ty else {
            return Ok(());
        };
        let declared = type_name(declaration);
        let actual = match constant(default) {
            Some(Constant::Null) => return Ok(()),
            Some(Constant::Bool(true)) => "true",
            Some(Constant::Bool(false)) => "false",
            Some(Constant::Int(_)) => "int",
            Some(Constant::Float(_)) => "float",
            Some(Constant::Bytes(_)) => "string",
            Some(Constant::EmptyArray) => "array",
            None if matches!(default.kind, ExpressionKind::Array(_)) => "array",
            None => return Ok(()),
        };
        let accepted = declared
            .trim_start_matches('?')
            .split('|')
            .any(|candidate| {
                candidate == actual
                    || candidate == "mixed"
                    || candidate == "float" && actual == "int"
                    || candidate == "bool" && matches!(actual, "true" | "false")
                    || candidate == "iterable" && actual == "array"
            });
        if accepted {
            return Ok(());
        }
        let actual = if matches!(actual, "true" | "false") {
            "bool"
        } else {
            actual
        };
        Err(self.error(
            format!(
                "Cannot use {actual} as default value for parameter ${} of type {declared}",
                parameter.name
            ),
            line,
        ))
    }
}

/// Visit every initializer operand without recursing through long PHP expressions.
pub(crate) fn constant_expression(expression: &Expression) -> bool {
    initializer_error(expression, false).is_none()
}

/// Constructors are allowed in defaults for parameters and in attribute arguments.
pub(crate) fn initializer_error(
    expression: &Expression,
    allow_dynamic: bool,
) -> Option<&'static str> {
    use ExpressionKind::*;
    use phera_ast::{Arguments, MemberName, NewTarget};

    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        match &expression.kind {
            Literal(_) | Name(_) | MagicConstant(_) => {}
            ClassConstant { class, name } => {
                match &class.kind {
                    Name(class) if class.text.eq_ignore_ascii_case("static") => {
                        return Some(
                            if matches!(name, MemberName::Identifier(member) if member.name.eq_ignore_ascii_case("class"))
                            {
                                "static::class cannot be used for compile-time class name resolution"
                            } else {
                                "\"static::\" is not allowed in compile-time constants"
                            },
                        );
                    }
                    Name(_) => {}
                    _ => return Some("Cannot use dynamic class name in constant expression"),
                }
                if let MemberName::Expression(name) = name {
                    pending.push(name);
                }
            }
            Property {
                target,
                name,
                nullsafe: false,
            } => {
                pending.push(target);
                if let MemberName::Expression(name) = name {
                    pending.push(name);
                }
            }
            Call {
                callee,
                arguments: Arguments::FirstClassCallable,
                ..
            } if matches!(
                callee.kind,
                Name(_) | Literal(phera_ast::Literal::String(_))
            ) => {}
            StaticCall {
                class,
                name: MemberName::Identifier(_),
                arguments: Arguments::FirstClassCallable,
                ..
            } if matches!(class.kind, Name(_)) => {}
            Closure(closure) if closure.is_static && closure.uses.is_empty() => {}
            New {
                class: NewTarget::Class(class),
                arguments: Arguments::List(arguments),
            } if allow_dynamic && matches!(class.kind, Name(_)) => {
                if matches!(&class.kind, Name(name) if name.text.eq_ignore_ascii_case("static")) {
                    return Some("\"static\" is not allowed in compile-time constants");
                }
                for argument in arguments.iter().rev() {
                    if argument.spread {
                        return Some("Argument unpacking in constant expressions is not supported");
                    }
                    pending.push(&argument.value);
                }
            }
            Array(items) => {
                for item in items.iter().rev() {
                    if item.by_reference {
                        return Some("Cannot use references in constant expression");
                    }
                    pending.push(&item.value);
                    pending.extend(item.key.as_ref());
                }
            }
            Unary { operator, operand } if *operator != phera_ast::UnaryOperator::ErrorSuppress => {
                pending.push(operand)
            }
            Cast { kind, operand } => {
                if !allow_dynamic && *kind == phera_ast::CastKind::Object {
                    return Some("Object casts are not supported in this context");
                }
                pending.push(operand);
            }
            Ternary {
                condition,
                then,
                otherwise,
            } => {
                pending.push(otherwise);
                pending.extend(then.as_deref());
                pending.push(condition);
            }
            Index {
                target,
                index: Some(index),
            }
            | Binary {
                left: target,
                right: index,
                ..
            } => {
                pending.push(index);
                pending.push(target);
            }
            _ => return Some("Constant expression contains invalid operations"),
        }
    }
    None
}

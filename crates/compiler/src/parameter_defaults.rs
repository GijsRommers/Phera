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
        if !parameter_expression(default) {
            return Err(self.error("Constant expression contains invalid operations", line));
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

/// Defaults permit constant arithmetic and arrays, but never variables or function calls.
pub(crate) fn constant_expression(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Literal(_) | ExpressionKind::Name(_) | ExpressionKind::MagicConstant(_) => {
            true
        }
        ExpressionKind::ClassConstant { .. } => true,
        ExpressionKind::Call {
            callee,
            arguments: phera_ast::Arguments::FirstClassCallable,
        } => {
            matches!(
                callee.kind,
                ExpressionKind::Name(_) | ExpressionKind::Literal(phera_ast::Literal::String(_))
            )
        }
        ExpressionKind::StaticCall {
            class,
            name: phera_ast::MemberName::Identifier(_),
            arguments: phera_ast::Arguments::FirstClassCallable,
        } => {
            matches!(class.kind, ExpressionKind::Name(_))
        }
        ExpressionKind::Closure(closure) => closure.is_static && closure.uses.is_empty(),
        ExpressionKind::ArrowFunction(_) => false,
        ExpressionKind::Array(items) => items.iter().all(|item| {
            !item.by_reference
                && item.key.as_ref().is_none_or(constant_expression)
                && constant_expression(&item.value)
        }),
        ExpressionKind::Unary { operator, operand } => {
            *operator != phera_ast::UnaryOperator::ErrorSuppress && constant_expression(operand)
        }
        ExpressionKind::Ternary {
            condition,
            then,
            otherwise,
        } => {
            constant_expression(condition)
                && then.as_deref().is_none_or(constant_expression)
                && constant_expression(otherwise)
        }
        ExpressionKind::Index { target, index } => {
            constant_expression(target) && index.as_deref().is_some_and(constant_expression)
        }
        ExpressionKind::Binary { left, right, .. } => {
            constant_expression(left) && constant_expression(right)
        }
        _ => false,
    }
}

/// PHP permits named-class construction in parameter defaults but not property defaults.
fn parameter_expression(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::New {
            class: phera_ast::NewTarget::Class(class),
            arguments: phera_ast::Arguments::List(arguments),
        } => {
            matches!(class.kind, ExpressionKind::Name(_))
                && arguments
                    .iter()
                    .all(|argument| !argument.spread && parameter_expression(&argument.value))
        }
        ExpressionKind::Array(items) => items.iter().all(|item| {
            !item.by_reference
                && item.key.as_ref().is_none_or(parameter_expression)
                && parameter_expression(&item.value)
        }),
        _ => constant_expression(expression),
    }
}

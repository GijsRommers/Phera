//! `trait T { public int $value = 'text'; }` fails before any PHP code executes.

use phera_ast::{Expression, ExpressionKind, Type};

use crate::{CompileError, Compiler, constant, ir::Constant};

#[derive(Clone, Copy)]
pub(super) enum DefaultKind {
    Property,
    Constant,
}

impl Compiler<'_> {
    pub(super) fn validate_literal_member_default(
        &self,
        expression: &Expression,
        ty: &Type,
        name: &str,
        kind: DefaultKind,
        line: usize,
    ) -> Result<(), CompileError> {
        let actual = match constant(expression) {
            Some(Constant::Null) => "null",
            Some(Constant::Bool(true)) => "true",
            Some(Constant::Bool(false)) => "false",
            Some(Constant::Int(_)) => "int",
            Some(Constant::Float(_)) => "float",
            Some(Constant::Bytes(_)) => "string",
            Some(Constant::EmptyArray) => "array",
            None if matches!(expression.kind, ExpressionKind::Array(_)) => "array",
            None => return Ok(()),
        };
        let declared = self.declaration_type(ty);
        let accepted = actual == "null" && declared.starts_with('?')
            || declared.trim_start_matches('?').split('|').any(|member| {
                member == actual
                    || member == "mixed"
                    || member == "float" && actual == "int"
                    || member == "bool" && matches!(actual, "true" | "false")
                    || member == "iterable" && actual == "array"
            });
        if accepted {
            return Ok(());
        }
        let actual = if matches!(actual, "true" | "false") {
            "bool"
        } else {
            actual
        };
        let message = match (kind, actual) {
            (DefaultKind::Constant, _) => {
                format!("Cannot use {actual} as value for class constant {name} of type {declared}")
            }
            (DefaultKind::Property, "null") => {
                let nullable = if declared.contains('|') {
                    format!("{declared}|null")
                } else if declared.contains('&') {
                    format!("({declared})|null")
                } else {
                    format!("?{declared}")
                };
                format!(
                    "Default value for property of type {declared} may not be null. Use the nullable type {nullable} to allow null default value"
                )
            }
            (DefaultKind::Property, _) => {
                format!(
                    "Cannot use {actual} as default value for property {name} of type {declared}"
                )
            }
        };
        Err(self.error(message, line))
    }
}

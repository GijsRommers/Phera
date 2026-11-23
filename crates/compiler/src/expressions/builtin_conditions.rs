//! `is_null(...)` as the condition of a branch.
//!
//! ```php
//! if (is_null($value)) { ... }        // BranchNotNull $value, else
//! namespace App;
//! if (!is_null($this->cache)) { ... } // GuardBuiltin, BranchNull, or the call
//! ```
//!
//! The null test is the branch itself (`BranchNull` / `BranchNotNull`), as
//! for `$value === null`: no boolean is made and tested. Inside a namespace
//! the guard of `namespaced_builtins` stands in front, and its general call
//! branches on the call's result.

use phera_ast::{Expression, ExpressionKind};
use phera_bytecode::{BuiltinUnary, TypeMask};

use crate::ir::{Instruction, Target};
use crate::policies::NameKind;
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// The branch for a condition that is a call of the built-in `is_null`
    /// with one plain argument, or `None` for any other condition. Every
    /// returned position jumps to the caller's target when the condition's
    /// truthiness equals `when`.
    pub(crate) fn compile_builtin_condition(
        &mut self,
        expression: &Expression,
        when: bool,
        line: usize,
    ) -> Result<Option<Vec<usize>>, CompileError> {
        let ExpressionKind::Call {
            callee, arguments, ..
        } = &expression.kind
        else {
            return Ok(None);
        };
        let ExpressionKind::Name(name) = &callee.kind else {
            return Ok(None);
        };
        if callee.span != name.span {
            return Ok(None);
        }
        let (resolved, fallback) = self.resolve(name, NameKind::Function);
        let global = fallback.as_deref().unwrap_or(&resolved);
        let Some((operator, argument)) = self.guarded_operation(global, arguments) else {
            return Ok(None);
        };
        if operator != BuiltinUnary::TypeCheck(TypeMask::NULL) {
            return Ok(None);
        }
        let null_test = |value, target| {
            if when {
                Instruction::BranchNull { value, target }
            } else {
                Instruction::BranchNotNull { value, target }
            }
        };
        let Some(fallback) = fallback else {
            let value = self.builtin_operand(&argument.value, line)?;
            return Ok(Some(vec![self.emit(null_test(value, Target(0)), line)]));
        };
        let guard = self.emit(
            Instruction::GuardBuiltin {
                name: resolved.clone(),
                fallback: Some(fallback.clone()),
                call: Target(0),
            },
            line,
        );
        let value = self.builtin_operand(&argument.value, line)?;
        let builtin = self.emit(null_test(value, Target(0)), line);
        let done = self.emit(Instruction::Jump(Target(0)), line);
        self.patch(guard, self.position());
        self.emit(
            Instruction::StartCall {
                name: resolved,
                fallback: Some(fallback),
            },
            line,
        );
        let result = self.compile_arguments(arguments, line)?;
        let call = self.emit(
            Instruction::Branch {
                value: result,
                target: Target(0),
                when,
            },
            line,
        );
        self.patch(done, self.position());
        Ok(Some(vec![builtin, call]))
    }
}

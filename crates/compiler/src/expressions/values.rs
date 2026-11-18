//! Lower AST expression constructs to IR while preserving PHP evaluation order.

use phera_ast as ast;
use phera_bytecode::{FetchMode, Reg};

use crate::ir::{Constant, Instruction, Target, Unary};
use crate::policies::NameKind;
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Use zero when exit has no explicit argument.
    pub(crate) fn compile_exit(
        &mut self,
        expression: &Option<Box<ast::Expression>>,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let register = if let Some(expression) = expression {
            self.compile_expression(expression)?
        } else {
            self.literal(Constant::Int(0), line)?
        };
        self.emit(Instruction::Exit(register), line);
        Ok(register)
    }

    /// Resolve namespace fallback before loading a named constant.
    pub(crate) fn compile_constant(
        &mut self,
        name: &ast::Name,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let (name, fallback) = self.resolve(name, NameKind::Constant);
        match name.to_ascii_lowercase().as_str() {
            "true" => return self.literal(Constant::Bool(true), line),
            "false" => return self.literal(Constant::Bool(false), line),
            "null" => return self.literal(Constant::Null, line),
            _ => {}
        }
        let dst = self.allocate_register()?;
        self.emit(
            Instruction::Constant {
                destination: dst,
                name,
                fallback,
            },
            line,
        );
        Ok(dst)
    }

    /// Quiet reads suppress undefined-variable warnings for empty.
    pub(crate) fn compile_empty(
        &mut self,
        value: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let src = self.quiet_access(value, FetchMode::Empty)?;
        let dst = self.allocate_register()?;
        self.emit(
            Instruction::Unary {
                destination: dst,
                operator: Unary::Not,
                source: src,
            },
            line,
        );
        Ok(dst)
    }

    /// Stop at the first null value without raising undefined-variable warnings.
    pub(crate) fn compile_isset(
        &mut self,
        values: &[ast::Expression],
        line: usize,
    ) -> Result<Reg, CompileError> {
        let dst = self.literal(Constant::Bool(false), line)?;
        let mut ends = vec![];
        for value in values {
            let register = self.quiet_access(value, FetchMode::Isset)?;
            ends.push(self.emit(
                Instruction::BranchNull {
                    value: register,
                    target: Target(0),
                },
                line,
            ));
        }
        let src = self.literal(Constant::Bool(true), line)?;
        self.emit(
            Instruction::Move {
                destination: dst,
                source: src,
            },
            line,
        );
        for jump in ends {
            self.patch(jump, self.position())
        }
        Ok(dst)
    }
}

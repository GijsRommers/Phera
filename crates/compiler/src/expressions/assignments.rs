//! Lower AST expression constructs to IR while preserving PHP evaluation order.

use phera_ast as ast;
use phera_ast::ExpressionKind;
use phera_bytecode::Reg;

use crate::ir::{Instruction, Target};
use crate::{CompileError, Compiler, binary, writable};

impl Compiler<'_> {
    /// Read a variable or indexed value after evaluating its path.
    pub(crate) fn compile_read(
        &mut self,
        expression: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let place = self.readable_place(expression)?;
        let dst = self.allocate_register()?;
        self.emit(
            Instruction::Read {
                dst,
                place,
                mode: phera_bytecode::FetchMode::Read,
            },
            line,
        );
        Ok(dst)
    }

    /// Evaluate the source before writing, retaining references in destructuring.
    pub(crate) fn compile_assignment(
        &mut self,
        target: &ast::Expression,
        value: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let references = matches!(&target.kind, ExpressionKind::List(items) if items.iter().flatten().any(|item| item.by_reference));
        let src = if references && writable(value) {
            let place = self.place(value)?;
            let dst = self.allocate_register()?;
            self.emit(Instruction::Reference { dst, place }, line);
            dst
        } else {
            self.compile_expression(value)?
        };
        self.assign(target, src, line)?;
        Ok(src)
    }

    /// Bind references while retaining the original target evaluation order.
    pub(crate) fn compile_reference_assignment(
        &mut self,
        target: &ast::Expression,
        value: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let place = self.place(target)?;
        if matches!(value.kind, ast::ExpressionKind::Call { .. }) {
            let src = self.compile_expression(value)?;
            self.emit(Instruction::Rebind { place, src }, line);
            return Ok(src);
        }
        let source = self.place(value)?;
        self.emit(
            Instruction::Bind {
                place: place.clone(),
                source,
            },
            line,
        );
        let dst = self.allocate_register()?;
        self.emit(
            Instruction::Read {
                dst,
                place,
                mode: phera_bytecode::FetchMode::Read,
            },
            line,
        );
        Ok(dst)
    }

    /// Evaluate the place once before applying a compound assignment.
    pub(crate) fn compile_compound_assignment(
        &mut self,
        operator: &ast::BinaryOperator,
        target: &ast::Expression,
        value: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        if *operator != ast::BinaryOperator::NullCoalesce {
            let place = self.place(target)?;
            let source = self.compile_expression(value)?;
            let dst = self.allocate_register()?;
            let operator =
                binary(*operator).ok_or_else(|| self.error("Operator not supported yet", line))?;
            self.emit(
                Instruction::Compound {
                    dst,
                    place,
                    operator,
                    source,
                },
                line,
            );
            return Ok(dst);
        }
        self.compile_coalescing_assignment(target, value, line)
    }

    /// Read the target quietly and evaluate the right side only when it is null.
    fn compile_coalescing_assignment(
        &mut self,
        target: &ast::Expression,
        value: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let place = self.place(target)?;
        let lhs = self.allocate_register()?;
        self.emit(
            Instruction::Read {
                dst: lhs,
                place: place.clone(),
                mode: phera_bytecode::FetchMode::Quiet,
            },
            line,
        );
        let jump = self.emit(
            Instruction::BranchNull {
                value: lhs,
                target: Target(0),
            },
            line,
        );
        let end = self.emit(Instruction::Jump(Target(0)), line);
        self.patch(jump, self.position());
        let rhs = self.compile_expression(value)?;
        self.emit(Instruction::Write { place, src: rhs }, line);
        self.emit(Instruction::Move { dst: lhs, src: rhs }, line);
        self.patch(end, self.position());
        Ok(lhs)
    }
}

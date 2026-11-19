//! Lower AST expression constructs to IR while preserving PHP evaluation order.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Argument, Instruction, Target, Unary};
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Preserve the value selected by prefix or postfix increment.
    pub(crate) fn compile_increment(
        &mut self,
        target: &ast::Expression,
        mode: phera_bytecode::IncDec,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let place = self.place(target)?;
        let dst = self.allocate_register()?;
        self.emit(
            Instruction::IncPlace {
                destination: dst,
                place,
                mode,
            },
            line,
        );
        Ok(dst)
    }

    /// Evaluate the pipe input before its callable.
    pub(crate) fn compile_pipe(
        &mut self,
        left: &ast::Expression,
        right: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let value = self.compile_expression(left)?;
        let name = self.compile_expression(right)?;
        let dst = self.allocate_register()?;
        self.emit(
            Instruction::Call {
                result: phera_bytecode::CallResult::Used,
                destination: dst,
                name,
                fallback: None,
                arguments: vec![Argument {
                    source: crate::ir::ArgumentSource::Value(value),
                    name: None,
                    spread: false,
                }],
            },
            line,
        );
        Ok(dst)
    }

    /// Skip the right operand according to PHP short-circuit rules.
    pub(crate) fn compile_short_circuit(
        &mut self,
        operator: &ast::BinaryOperator,
        left: &ast::Expression,
        right: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let lhs = if *operator == ast::BinaryOperator::NullCoalesce {
            self.quiet(left)?
        } else {
            self.compile_expression(left)?
        };
        let dst = self.allocate_register()?;
        self.emit(
            Instruction::Move {
                destination: dst,
                source: lhs,
            },
            line,
        );
        let jump = if *operator == ast::BinaryOperator::NullCoalesce {
            let jump = self.emit(
                Instruction::BranchNull {
                    value: lhs,
                    target: Target(0),
                },
                line,
            );
            let end = self.emit(Instruction::Jump(Target(0)), line);
            self.patch(jump, self.position());
            end
        } else {
            self.emit(
                Instruction::Branch {
                    value: lhs,
                    target: Target(0),
                    when: *operator == ast::BinaryOperator::Or,
                },
                line,
            )
        };
        let rhs = self.compile_expression(right)?;
        self.emit(
            Instruction::Move {
                destination: dst,
                source: rhs,
            },
            line,
        );
        self.patch(jump, self.position());
        if *operator != ast::BinaryOperator::NullCoalesce {
            self.emit(
                Instruction::Unary {
                    destination: dst,
                    operator: Unary::Bool,
                    source: dst,
                },
                line,
            );
        }
        Ok(dst)
    }

    /// Bracket suppressed expressions with silence instructions.
    pub(crate) fn compile_unary(
        &mut self,
        operator: &ast::UnaryOperator,
        operand: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        if *operator == ast::UnaryOperator::ErrorSuppress {
            self.emit(Instruction::Suppress(true), line);
            let register = self.compile_expression(operand)?;
            self.emit(Instruction::Suppress(false), line);
            return Ok(register);
        }
        let src = self.compile_expression(operand)?;
        let dst = self.allocate_register()?;
        let operator = match operator {
            ast::UnaryOperator::Plus => Unary::Plus,
            ast::UnaryOperator::Minus => Unary::Minus,
            ast::UnaryOperator::Not => Unary::Not,
            _ => Unary::BitwiseNot,
        };
        self.emit(
            Instruction::Unary {
                destination: dst,
                operator,
                source: src,
            },
            line,
        );
        Ok(dst)
    }
}

//! Iterative expression-spine lowering avoids Rust recursion for long PHP chains.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Argument, Instruction, Target, Unary};
use crate::{CompileError, Compiler, binary};
/// One deferred right operand, retaining its operator and source location.
struct BinaryStep<'ast> {
    operator: ast::BinaryOperator,
    right: &'ast ast::Expression,
    line: usize,
}

impl Compiler<'_> {
    /// Walk long left-associated binary chains without growing the Rust call stack.
    pub(crate) fn compile_expression(
        &mut self,
        expression: &ast::Expression,
    ) -> Result<Reg, CompileError> {
        if matches!(expression.kind, ast::ExpressionKind::Binary { .. }) {
            let mut spine = vec![];
            let mut node = expression;
            while let ast::ExpressionKind::Binary {
                operator,
                left,
                right,
            } = &node.kind
            {
                spine.push(BinaryStep {
                    operator: *operator,
                    right,
                    line: self.lines.line(node.span.start),
                });
                node = left;
            }
            let mut lhs = if spine
                .last()
                .is_some_and(|step| step.operator == ast::BinaryOperator::NullCoalesce)
            {
                self.quiet(node)?
            } else {
                self.compile_expression(node)?
            };
            for BinaryStep {
                operator,
                right,
                line,
            } in spine.into_iter().rev()
            {
                let watermark = self.next;
                match operator {
                    ast::BinaryOperator::And
                    | ast::BinaryOperator::Or
                    | ast::BinaryOperator::NullCoalesce => {
                        lhs = self.compile_short_circuit_step(operator, lhs, right, line)?;
                    }
                    ast::BinaryOperator::Pipe => {
                        lhs = self.compile_pipe_step(operator, lhs, right, line)?;
                    }
                    _ => {
                        let rhs = self.compile_expression(right)?;
                        self.emit(
                            Instruction::Binary {
                                destination: lhs,
                                operator: binary(operator).ok_or_else(|| {
                                    self.error("Operator not supported yet", line)
                                })?,
                                left: lhs,
                                right: rhs,
                            },
                            line,
                        );
                    }
                }
                if self
                    .locals
                    .values()
                    .all(|register| (register.0 as usize) < watermark)
                {
                    self.next = watermark;
                }
            }
            return Ok(lhs);
        }
        self.compile_expression_node(expression)
    }
    /// Evaluate one deferred chain operand without recursing down the left spine.
    fn compile_short_circuit_step(
        &mut self,
        operator: ast::BinaryOperator,
        lhs: Reg,
        right: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let dst = lhs;
        let jump = if operator == ast::BinaryOperator::NullCoalesce {
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
                    when: operator == ast::BinaryOperator::Or,
                },
                line,
            )
        };
        let src = self.compile_expression(right)?;
        self.emit(
            Instruction::Move {
                destination: dst,
                source: src,
            },
            line,
        );
        self.patch(jump, self.position());
        if operator != ast::BinaryOperator::NullCoalesce {
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

    /// Evaluate one deferred chain operand without recursing down the left spine.
    fn compile_pipe_step(
        &mut self,
        _operator: ast::BinaryOperator,
        lhs: Reg,
        right: &ast::Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let name = self.compile_expression(right)?;
        let dst = lhs;
        self.emit(
            Instruction::Call {
                result: phera_bytecode::CallResult::Used,
                destination: dst,
                name,
                fallback: None,
                arguments: vec![Argument {
                    source: crate::ir::ArgumentSource::Value(lhs),
                    name: None,
                    spread: false,
                }],
            },
            line,
        );
        Ok(dst)
    }
}

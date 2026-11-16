//! Iterative expression-spine lowering avoids Rust recursion for long PHP chains.
use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Argument, Instruction, Target, Unary};
use crate::{CompileError, Compiler, binary};
impl Compiler<'_> {
    pub(super) fn expr(&mut self, expression: &ast::Expression) -> Result<Reg, CompileError> {
        if matches!(expression.kind, ast::ExpressionKind::Binary { .. }) {
            let mut spine = vec![];
            let mut node = expression;
            while let ast::ExpressionKind::Binary {
                operator,
                left,
                right,
            } = &node.kind
            {
                spine.push((*operator, right.as_ref(), self.lines.line(node.span.start)));
                node = left;
            }
            let mut lhs = if spine
                .last()
                .is_some_and(|(operator, _, _)| *operator == ast::BinaryOperator::NullCoalesce)
            {
                self.quiet(node)?
            } else {
                self.expr(node)?
            };
            for (operator, right, line) in spine.into_iter().rev() {
                let watermark = self.next;
                match operator {
                    ast::BinaryOperator::And
                    | ast::BinaryOperator::Or
                    | ast::BinaryOperator::NullCoalesce => {
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
                            self.patch(jump, self.pos());
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
                        let src = self.expr(right)?;
                        self.emit(Instruction::Move { dst, src }, line);
                        self.patch(jump, self.pos());
                        if operator != ast::BinaryOperator::NullCoalesce {
                            self.emit(
                                Instruction::Unary {
                                    dst,
                                    operator: Unary::Bool,
                                    src: dst,
                                },
                                line,
                            );
                        }
                        lhs = dst;
                    }
                    ast::BinaryOperator::Pipe => {
                        let name = self.expr(right)?;
                        let dst = lhs;
                        self.emit(
                            Instruction::Call {
                                dst,
                                name,
                                fallback: None,
                                arguments: vec![Argument {
                                    value: lhs,
                                    place: None,
                                    name: None,
                                    spread: false,
                                }],
                            },
                            line,
                        );
                        lhs = dst;
                    }
                    _ => {
                        let rhs = self.expr(right)?;
                        self.emit(
                            Instruction::Binary {
                                dst: lhs,
                                operator: binary(operator).ok_or_else(|| {
                                    self.error("Operator not supported yet", line)
                                })?,
                                lhs,
                                rhs,
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
        self.expr_node(expression)
    }
}

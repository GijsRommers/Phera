//! Array construction and match/interpolation expression lowering.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Binary, Instruction, Target};
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Compare conditions strictly and evaluate only the selected arm body.
    pub(crate) fn compile_match(
        &mut self,
        match_expression: &ast::Match,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let subject = self.compile_expression(&match_expression.subject)?;
        let dst = self.allocate_register()?;
        let mut done = vec![];
        let mut default = None;
        for arm in &match_expression.arms {
            if let Some(conditions) = &arm.conditions {
                let mut jumps = vec![];
                for condition in conditions {
                    let rhs = self.compile_expression(condition)?;
                    let test = self.allocate_register()?;
                    self.emit(
                        Instruction::Binary {
                            destination: test,
                            operator: Binary::Identical,
                            left: subject,
                            right: rhs,
                        },
                        line,
                    );
                    jumps.push(self.emit(
                        Instruction::Branch {
                            value: test,
                            target: Target(0),
                            when: true,
                        },
                        line,
                    ));
                }
                let skip = self.emit(Instruction::Jump(Target(0)), line);
                for jump in jumps {
                    self.patch(jump, self.position())
                }
                let src = self.compile_expression(&arm.body)?;
                self.emit(
                    Instruction::Move {
                        destination: dst,
                        source: src,
                    },
                    line,
                );
                done.push(self.emit(Instruction::Jump(Target(0)), line));
                self.patch(skip, self.position());
            } else {
                default = Some(&arm.body)
            }
        }
        if let Some(expression) = default {
            let src = self.compile_expression(expression)?;
            self.emit(
                Instruction::Move {
                    destination: dst,
                    source: src,
                },
                line,
            );
        } else {
            self.emit(Instruction::MatchError { subject }, line);
        }
        for jump in done {
            self.patch(jump, self.position())
        }
        Ok(dst)
    }
}

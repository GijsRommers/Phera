//! Array construction and match/interpolation expression lowering.
use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Binary, Instruction, Target};
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    pub(super) fn compile_match(
        &mut self,
        match_expression: &ast::Match,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let subject = self.expr(&match_expression.subject)?;
        let dst = self.reg()?;
        let mut done = vec![];
        let mut default = None;
        for arm in &match_expression.arms {
            if let Some(conditions) = &arm.conditions {
                let mut jumps = vec![];
                for condition in conditions {
                    let rhs = self.expr(condition)?;
                    let test = self.reg()?;
                    self.emit(
                        Instruction::Binary {
                            dst: test,
                            operator: Binary::Identical,
                            lhs: subject,
                            rhs,
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
                    self.patch(jump, self.pos())
                }
                let src = self.expr(&arm.body)?;
                self.emit(Instruction::Move { dst, src }, line);
                done.push(self.emit(Instruction::Jump(Target(0)), line));
                self.patch(skip, self.pos());
            } else {
                default = Some(&arm.body)
            }
        }
        if let Some(expression) = default {
            let src = self.expr(expression)?;
            self.emit(Instruction::Move { dst, src }, line);
        } else {
            self.emit(Instruction::MatchError { subject }, line);
        }
        for jump in done {
            self.patch(jump, self.pos())
        }
        Ok(dst)
    }
}

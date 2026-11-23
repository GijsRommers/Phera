//! A `match ($value)` comparison fetches the subject after evaluating its arm.

use phera_ast as ast;
use phera_bytecode::Reg;
use phera_value::Value;

use super::match_constants::match_constant;
use crate::ir::{Binary, Instruction, Target};
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Compare conditions strictly and evaluate only the selected arm body.
    pub(crate) fn compile_match(
        &mut self,
        match_expression: &ast::Match,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let local = self.compiled_variable(&match_expression.subject)?;
        // Zend's jump-table form fetches the CV once. Sequential comparisons
        // instead reuse the CV after evaluating each arm's condition.
        let subject = match local {
            Some(local) if !has_jump_table(match_expression) => local,
            _ => self.compile_expression(&match_expression.subject)?,
        };
        let dst = self.allocate_register()?;
        let mut done = vec![];
        let mut default = None;
        let subject_line = self.lines.line(match_expression.subject.span.start);
        let mut checking_jump_table = true;
        for arm in &match_expression.arms {
            if let Some(conditions) = &arm.conditions {
                let mut jumps = vec![];
                for condition in conditions {
                    let mark = self.mark_temporaries();
                    let rhs = self.compile_expression(condition)?;
                    let test = self.allocate_register()?;
                    let test_line = if Some(subject) == local {
                        let folded = checking_jump_table
                            .then(|| match_constant(condition))
                            .flatten();
                        checking_jump_table = matches!(
                            folded,
                            Some(Value::Int(_) | Value::InternedStr(_) | Value::String(_))
                        );
                        // The jump-table probe folds constants with the subject's
                        // line. PHP's scalar literal nodes keep their own lines.
                        if folded.is_some()
                            && !matches!(
                                condition.kind,
                                ast::ExpressionKind::Literal(
                                    ast::Literal::Int(_)
                                        | ast::Literal::Float(_)
                                        | ast::Literal::String(_)
                                )
                            )
                        {
                            subject_line
                        } else {
                            self.operand_line(condition)
                        }
                    } else {
                        line
                    };
                    self.emit(
                        Instruction::Binary {
                            destination: test,
                            operator: Binary::Identical,
                            left: subject,
                            right: rhs,
                        },
                        test_line,
                    );
                    jumps.push(self.emit(
                        Instruction::Branch {
                            value: test,
                            target: Target(0),
                            when: true,
                        },
                        line,
                    ));
                    self.release_temporaries(mark);
                }
                let skip = self.emit(Instruction::Jump(Target(0)), line);
                for jump in jumps {
                    self.patch(jump, self.position())
                }
                let mark = self.mark_temporaries();
                let src = self.compile_expression(&arm.body)?;
                self.emit(
                    Instruction::Move {
                        destination: dst,
                        source: src,
                    },
                    line,
                );
                self.release_temporaries(mark);
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
            self.emit(
                Instruction::MatchError {
                    subject: local.unwrap_or(subject),
                },
                line,
            );
        }
        for jump in done {
            self.patch(jump, self.position())
        }
        Ok(dst)
    }
}

fn has_jump_table(expression: &ast::Match) -> bool {
    let mut count = 0;
    for condition in expression
        .arms
        .iter()
        .filter_map(|arm| arm.conditions.as_ref())
        .flatten()
    {
        if !matches!(
            match_constant(condition),
            Some(Value::Int(_) | Value::InternedStr(_) | Value::String(_))
        ) {
            return false;
        }
        count += 1;
    }
    count >= 2
}

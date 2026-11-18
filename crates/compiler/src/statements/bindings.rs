//! Compile statement constructs into ordered IR and patch their control-flow exits.

use phera_ast as ast;

use crate::ir::{Constant, Instruction, Target};
use crate::policies::DeclarationTiming;
use crate::{CompileError, Compiler, Loop};

impl Compiler<'_> {
    /// Evaluate each writable path before unsetting it.
    pub(crate) fn compile_unset(
        &mut self,
        expressions: &[ast::Expression],
        line: usize,
    ) -> Result<(), CompileError> {
        for expression in expressions {
            let parameter = self.place(expression)?;
            if parameter
                .path
                .iter()
                .any(|step| matches!(step, crate::ir::PathStep::Append))
            {
                return Err(self.error("Cannot use [] for unsetting", line));
            }
            self.emit(Instruction::Unset(parameter), line);
        }

        Ok(())
    }

    /// Keep reference iteration attached to the original writable subject.
    pub(crate) fn compile_foreach(
        &mut self,
        subject: &ast::Expression,
        key: &Option<ast::Expression>,
        value: &ast::Expression,
        binding: crate::policies::IterationBinding,
        body: &[ast::Statement],
        line: usize,
    ) -> Result<(), CompileError> {
        let reference_subject = binding == crate::policies::IterationBinding::Reference
            || crate::expressions::places::contains_reference(value);
        let mark = self.temporaries_start();
        let src = if reference_subject && crate::writable(subject) {
            self.literal(Constant::Null, line)?
        } else {
            self.compile_expression(subject)?
        };
        // The subject, and the registers the loop itself copies values into.
        let id = self.allocate_register()?;
        let key_register = key.as_ref().map(|_| self.allocate_register()).transpose()?;
        let value_register = self.allocate_register()?;
        let owned = self.temporaries_since(mark);
        let place = if reference_subject {
            Some(if crate::writable(subject) {
                self.place(subject)?
            } else {
                crate::ir::Place {
                    base: src,
                    scope: crate::ir::PlaceScope::Local,
                    path: vec![],
                }
            })
        } else {
            None
        };
        self.emit(
            Instruction::IterInit {
                iterator: id,
                source: src,
                place,
            },
            line,
        );
        let start = self.position();
        let jump = self.emit(
            Instruction::IterNext {
                iterator: id,
                key: key_register,
                value: value_register,
                end: Target(0),
            },
            line,
        );
        if binding == crate::policies::IterationBinding::Reference {
            let place = self.place(value)?;
            self.emit(
                Instruction::Rebind {
                    place,
                    source: value_register,
                },
                line,
            );
            self.emit(
                Instruction::Clear {
                    register: value_register,
                },
                line,
            );
        } else {
            self.assign(value, value_register, line)?;
            if reference_subject {
                self.emit(
                    Instruction::Clear {
                        register: value_register,
                    },
                    line,
                );
            }
        }
        // PHP publishes the value first, even when both targets are the same variable.
        if let (Some(key), Some(key_register)) = (key, key_register) {
            let place = self.place(key)?;
            self.emit(
                Instruction::Write {
                    place,
                    source: key_register,
                },
                line,
            );
        }
        self.loops.push(Loop {
            start: self.position(),
            cleanup: Some(super::exits::LoopCleanup::Foreach {
                iterator: id,
                owned,
            }),
            breaks: vec![],
            continues: vec![],
        });
        self.block(body, DeclarationTiming::AtExecution)?;
        self.emit(Instruction::Jump(Target(start)), line);
        self.patch(jump, self.position());
        self.loop_end(start);
        self.emit(Instruction::IterEnd(id), line);
        // PHP frees the loop's own copy of the subject and of the last
        // value when the loop ends (FE_FREE), so an object that only they
        // held is destroyed here.
        self.release_temporaries(&owned, line);

        Ok(())
    }
}

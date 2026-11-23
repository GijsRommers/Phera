//! Compile statement constructs into ordered IR and patch their control-flow exits.

use phera_ast as ast;
use phera_bytecode::Reg;

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
            if crate::helpers::is_this(expression) {
                return Err(self.error("Cannot unset $this", line));
            }
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
        // The subject, and the registers the loop writes entries into. A
        // plain local is written straight into its own register, skipping the
        // copy the temporary needed; references, list destructuring and
        // properties keep the copy. PHP publishes the value first, then the
        // key, so key and value on the same variable keep both copies.
        let by_value =
            binding != crate::policies::IterationBinding::Reference && !reference_subject;
        let mut value_local = if by_value {
            self.compiled_variable(value)?
        } else {
            None
        };
        let mut key_local = match key {
            Some(key) if by_value => self.compiled_variable(key)?,
            _ => None,
        };
        if let (Some(key), Some(value)) = (key_local, value_local)
            && key == value
        {
            key_local = None;
            value_local = None;
        }
        // The key is written straight only when the value is: a value that
        // is assigned afterwards (`[$a[$k], $b]`, `$box->value`) must still
        // see the key's old value, as PHP writes the value first.
        if value_local.is_none() {
            key_local = None;
        }
        let id = self.allocate_register()?;
        let key_register = match key_local {
            Some(local) => Some(local),
            None => key.as_ref().map(|_| self.allocate_register()).transpose()?,
        };
        let value_register = match value_local {
            Some(local) => local,
            None => self.allocate_register()?,
        };
        let owned = self.temporaries_since(mark);
        let subject_temporary = (!self.locals.values().any(|&local| local == src)).then_some(src);
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
        } else if value_local.is_none() {
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
        if let (Some(key), Some(key_register)) = (key, key_register)
            && key_local.is_none()
        {
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
                subject: subject_temporary,
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
        self.clear_foreach_temporaries(&owned, subject_temporary, line);

        Ok(())
    }

    pub(super) fn clear_foreach_temporaries(
        &mut self,
        owned: &crate::expressions::operand_release::Temporaries,
        subject: Option<Reg>,
        line: usize,
    ) {
        let mut registers = self.owning_temporaries(owned);
        // The body can overwrite the local from which the subject was copied.
        if let Some(subject) = subject
            && !registers.contains(&subject)
        {
            registers.insert(0, subject);
        }
        self.clear_registers(&registers, line);
    }
}

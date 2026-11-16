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
        let src = if binding == crate::policies::IterationBinding::Reference {
            self.literal(Constant::Null, line)?
        } else {
            self.compile_expression(subject)?
        };
        let id = self.allocate_register()?;
        let key_register = self.allocate_register()?;
        let value_register = self.allocate_register()?;
        let place = if binding == crate::policies::IterationBinding::Reference {
            Some(self.place(subject)?)
        } else {
            None
        };
        self.emit(Instruction::IterInit { id, src, place }, line);
        let start = self.position();
        let jump = self.emit(
            Instruction::IterNext {
                id,
                key: key_register,
                value: value_register,
                end: Target(0),
            },
            line,
        );
        if let Some(key) = key {
            let place = self.place(key)?;
            self.emit(
                Instruction::Write {
                    place,
                    src: key_register,
                },
                line,
            );
        }
        if binding == crate::policies::IterationBinding::Reference {
            let place = self.place(value)?;
            self.emit(
                Instruction::Rebind {
                    place,
                    src: value_register,
                },
                line,
            );
        } else {
            self.assign(value, value_register, line)?;
        }
        self.loops.push(Loop {
            breaks: vec![],
            continues: vec![],
        });
        self.block(body, DeclarationTiming::AtExecution)?;
        self.emit(Instruction::Jump(Target(start)), line);
        self.patch(jump, self.position());
        self.loop_end(start);

        Ok(())
    }
}

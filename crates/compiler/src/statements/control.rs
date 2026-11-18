//! Compile statement constructs into ordered IR and patch their control-flow exits.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Constant, Instruction, Target};
use crate::policies::DeclarationTiming;
use crate::{CompileError, Compiler, Loop};

impl Compiler<'_> {
    /// Release discarded temporaries after evaluating the expression.
    pub(crate) fn compile_expression_statement(
        &mut self,
        expression: &ast::Expression,
        line: usize,
    ) -> Result<(), CompileError> {
        let first = self.next;
        self.compile_expression(expression)?;
        self.clear_expression_temporaries(first, line);
        Ok(())
    }

    fn clear_expression_temporaries(&mut self, first: usize, line: usize) {
        let temporaries: Vec<_> = (first..self.next)
            .map(|index| Reg(index as u16))
            .filter(|register| !self.locals.values().any(|local| local == register))
            .filter(|register| !self.released.contains(&register.0))
            .collect();
        for register in temporaries {
            self.emit(Instruction::Clear { register }, line);
        }
    }

    /// Evaluate and print each argument in source order.
    pub(crate) fn compile_echo(
        &mut self,
        expressions: &[ast::Expression],
        line: usize,
    ) -> Result<(), CompileError> {
        for expression in expressions {
            let first = self.next;
            let register = self.compile_expression(expression)?;
            self.emit(Instruction::Echo(register), line);
            self.clear_expression_temporaries(first, line);
        }

        Ok(())
    }

    /// Emit literal output outside PHP tags.
    pub(crate) fn compile_inline_html(
        &mut self,
        value: &[u8],
        line: usize,
    ) -> Result<(), CompileError> {
        let register = self.literal(Constant::Bytes(value.to_vec()), line)?;
        self.emit(Instruction::Echo(register), line);

        Ok(())
    }

    /// Patch both branch exits after their bodies are emitted.
    pub(crate) fn compile_if(
        &mut self,
        condition: &ast::Expression,
        then: &[ast::Statement],
        otherwise: &Option<Vec<ast::Statement>>,
        line: usize,
    ) -> Result<(), CompileError> {
        let start = self.temporaries_start();
        let register = self.compile_expression(condition)?;
        let condition = self.temporaries_since(start);
        let jump = self.emit(
            Instruction::Branch {
                value: register,
                target: Target(0),
                when: false,
            },
            line,
        );
        self.release_temporaries(&condition, line);
        self.block(then, DeclarationTiming::AtExecution)?;
        let end = self.emit(Instruction::Jump(Target(0)), line);
        self.patch(jump, self.position());
        self.release_temporaries(&condition, line);
        if let Some(body) = otherwise {
            self.block(body, DeclarationTiming::AtExecution)?
        }
        self.patch(end, self.position());

        Ok(())
    }

    /// Continue returns to the condition; break exits the body.
    pub(crate) fn compile_while(
        &mut self,
        condition: &ast::Expression,
        body: &[ast::Statement],
        line: usize,
    ) -> Result<(), CompileError> {
        let start = self.position();
        let mark = self.temporaries_start();
        let register = self.compile_expression(condition)?;
        let condition = self.temporaries_since(mark);
        let end = self.emit(
            Instruction::Branch {
                value: register,
                target: Target(0),
                when: false,
            },
            line,
        );
        self.release_temporaries(&condition, line);
        self.loops.push(Loop {
            start: self.position(),
            cleanup: None,
            breaks: vec![],
            continues: vec![],
        });
        self.block(body, DeclarationTiming::AtExecution)?;
        self.emit(Instruction::Jump(Target(start)), line);
        self.patch(end, self.position());
        self.release_temporaries(&condition, line);
        self.loop_end(start);

        Ok(())
    }
}

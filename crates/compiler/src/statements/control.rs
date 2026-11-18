//! Compile statement constructs into ordered IR and patch their control-flow exits.

use phera_ast as ast;

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
        self.clear_temporaries_after(line, |compiler| {
            compiler.compile_expression(expression).map(drop)
        })
    }

    /// Evaluate and print each argument in source order.
    pub(crate) fn compile_echo(
        &mut self,
        expressions: &[ast::Expression],
        line: usize,
    ) -> Result<(), CompileError> {
        for expression in expressions {
            let mark = self.mark_temporaries();
            let register = self.compile_expression(expression)?;
            self.emit(Instruction::Echo(register), line);
            self.release_temporaries(mark);
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

    /// Test each condition in turn and run the first branch that holds;
    /// every branch then jumps past the rest of the ladder.
    pub(crate) fn compile_if(
        &mut self,
        condition: &ast::Expression,
        then: &[ast::Statement],
        elseifs: &[ast::ElseIf],
        otherwise: &Option<Vec<ast::Statement>>,
        line: usize,
    ) -> Result<(), CompileError> {
        let branches =
            std::iter::once((condition, then, line)).chain(elseifs.iter().map(|elseif| {
                (
                    &elseif.condition,
                    &elseif.body[..],
                    self.lines.line(elseif.span.start),
                )
            }));
        let mut ends = Vec::new();
        for (condition, body, line) in branches {
            let register = self.compile_expression(condition)?;
            let skip = self.emit(
                Instruction::Branch {
                    value: register,
                    target: Target(0),
                    when: false,
                },
                line,
            );
            self.block(body, DeclarationTiming::AtExecution)?;
            ends.push(self.emit(Instruction::Jump(Target(0)), line));
            self.patch(skip, self.position());
        }
        if let Some(body) = otherwise {
            self.block(body, DeclarationTiming::AtExecution)?
        }
        let end = self.position();
        for jump in ends {
            self.patch(jump, end);
        }

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
        let register = self.compile_expression(condition)?;
        let end = self.emit(
            Instruction::Branch {
                value: register,
                target: Target(0),
                when: false,
            },
            line,
        );
        self.loops.push(Loop {
            breaks: vec![],
            continues: vec![],
        });
        self.block(body, DeclarationTiming::AtExecution)?;
        self.emit(Instruction::Jump(Target(start)), line);
        self.patch(end, self.position());
        self.loop_end(start);

        Ok(())
    }
}

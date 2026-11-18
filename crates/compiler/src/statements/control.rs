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
        if let ast::ExpressionKind::Cast {
            kind: ast::CastKind::Void,
            operand,
        } = &expression.kind
        {
            self.compile_expression(operand)?;
        } else {
            let result = self.compile_expression(expression)?;
            if let Some(
                Instruction::FinishCall {
                    destination,
                    result: use_of_result,
                }
                | Instruction::Call {
                    destination,
                    result: use_of_result,
                    ..
                },
            ) = self.function.instructions.last_mut()
                && *destination == result
            {
                *use_of_result = phera_bytecode::CallResult::Discarded;
            }
        }
        let temporaries: Vec<_> = (first..self.next)
            .map(|index| Reg(index as u16))
            .filter(|register| !self.locals.values().any(|local| local == register))
            .collect();
        for register in temporaries {
            self.emit(Instruction::Clear { register }, line);
        }

        Ok(())
    }

    /// Evaluate and print each argument in source order.
    pub(crate) fn compile_echo(
        &mut self,
        expressions: &[ast::Expression],
        line: usize,
    ) -> Result<(), CompileError> {
        for expression in expressions {
            let register = self.compile_expression(expression)?;
            self.emit(Instruction::Echo(register), line);
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
        let register = self.compile_expression(condition)?;
        let jump = self.emit(
            Instruction::Branch {
                value: register,
                target: Target(0),
                when: false,
            },
            line,
        );
        self.block(then, DeclarationTiming::AtExecution)?;
        let end = self.emit(Instruction::Jump(Target(0)), line);
        self.patch(jump, self.position());
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

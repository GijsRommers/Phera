//! A yield publishes a value and evaluates to whatever the caller sends next.
//!
//! ```php
//! function receive() { $answer = yield 'question' => 1; return $answer; }
//! ```

use phera_ast::Expression;
use phera_bytecode::Reg;

use crate::{CompileError, Compiler, ir::Instruction};

impl Compiler<'_> {
    pub(super) fn compile_yield(
        &mut self,
        first: Option<&Expression>,
        second: Option<&Expression>,
        line: usize,
    ) -> Result<Reg, CompileError> {
        self.mark_generator(line)?;
        let (key, value) = match second {
            Some(value) => (first, Some(value)),
            None => (None, first),
        };
        let key = key
            .map(|expression| self.compile_expression(expression))
            .transpose()?;
        let value = value
            .map(|expression| self.compile_yield_value(expression, line))
            .transpose()?;
        let destination = self.allocate_register()?;
        self.emit(
            Instruction::Yield {
                destination,
                key,
                value,
            },
            line,
        );
        Ok(destination)
    }

    fn compile_yield_value(
        &mut self,
        expression: &Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        if self.function.returns_reference && crate::helpers::reference_capable(expression) {
            let place = self.readable_place(expression)?;
            let destination = self.allocate_register()?;
            self.emit(Instruction::Reference { destination, place }, line);
            return Ok(destination);
        }
        self.compile_expression(expression)
    }

    pub(super) fn compile_yield_from(
        &mut self,
        source: &Expression,
        line: usize,
    ) -> Result<Reg, CompileError> {
        self.mark_generator(line)?;
        if self.function.returns_reference {
            return Err(self.error(
                "Cannot use \"yield from\" inside a by-reference generator",
                line,
            ));
        }
        let source = self.compile_expression(source)?;
        let destination = self.allocate_register()?;
        self.emit(
            Instruction::YieldFrom {
                destination,
                source,
            },
            line,
        );
        Ok(destination)
    }

    fn mark_generator(&mut self, line: usize) -> Result<(), CompileError> {
        if self.function.is_generator {
            return Ok(());
        }
        if self.function.name == "{main}" {
            return Err(self.error(
                "The \"yield\" expression can only be used inside a function",
                line,
            ));
        }
        if let Some(ty) = &self.function.return_type
            && !ty.trim_start_matches('?').split('|').any(accepts_generator)
        {
            return Err(self.error(
                format!("Generator return type must be a supertype of Generator, {ty} given"),
                line,
            ));
        }
        self.function.is_generator = true;
        Ok(())
    }
}

fn accepts_generator(ty: &str) -> bool {
    matches!(
        ty.to_ascii_lowercase().as_str(),
        "generator" | "iterator" | "traversable" | "iterable" | "object" | "mixed"
    )
}

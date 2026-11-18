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
            && !accepts_generator(ty)
        {
            return Err(self.error(
                format!(
                    "Generator return type must be a supertype of Generator, {} given",
                    display_generator_type(ty),
                ),
                self.function.definition_line,
            ));
        }
        self.function.is_generator = true;
        Ok(())
    }
}

fn accepts_generator(declaration: &str) -> bool {
    // Zend checks the components of a top-level intersection or union for a
    // generator-compatible name. A union's intersection member is itself a type list.
    let separator = if declaration.contains('|') { '|' } else { '&' };
    declaration
        .trim_start_matches('?')
        .split(separator)
        .any(|member| {
            matches!(
                member.to_ascii_lowercase().as_str(),
                "generator" | "iterator" | "traversable" | "iterable" | "object" | "mixed"
            )
        })
}

fn display_generator_type(declaration: &str) -> String {
    if !declaration.contains('|') {
        return declaration.to_owned();
    }
    declaration
        .split('|')
        .map(|ty| {
            if ty.contains('&') {
                format!("({ty})")
            } else {
                ty.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("|")
}

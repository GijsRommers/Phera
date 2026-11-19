//! Callee-first call lowering and source-order argument sends.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Argument, Constant, Instruction};
use crate::policies::NameKind;
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Resolve the callee before sending arguments, since its signature governs references.
    pub(crate) fn compile_call(
        &mut self,
        callee: &ast::Expression,
        arguments: &ast::Arguments,
        line: usize,
    ) -> Result<Reg, CompileError> {
        use ast::ExpressionKind;
        if let ExpressionKind::Name(name) = &callee.kind
            && callee.span == name.span
            && matches!(
                name.text.to_ascii_lowercase().as_str(),
                "extract"
                    | "compact"
                    | "get_defined_vars"
                    | "func_num_args"
                    | "func_get_args"
                    | "func_get_arg"
            )
        {
            self.function.dynamic_scope = true;
        }
        let mut quoted_condition = None;
        if let ExpressionKind::Name(name) = &callee.kind
            && callee.span == name.span
        {
            quoted_condition = self.assertion_text(name, arguments);
            let (name, fallback) = self.resolve(name, NameKind::Function);
            self.emit(Instruction::StartCall { name, fallback }, line);
        } else {
            let callee = self.compile_expression(callee)?;
            self.emit(Instruction::StartDynamicCall { callee }, line);
        }
        let Some((description, name)) = quoted_condition else {
            return self.compile_arguments(arguments, line);
        };
        // `assert($x > 1)` is called as `assert($x > 1, 'assert($x > 1)')`:
        // the message of a failed assertion is the condition as written.
        if let ast::Arguments::List(arguments) = arguments {
            for argument in arguments {
                self.send_argument(argument, line)?;
            }
        }
        let description = self.literal(Constant::Bytes(description), line)?;
        self.emit(
            Instruction::Send {
                argument: Argument {
                    source: crate::ir::ArgumentSource::Value(description),
                    name,
                    spread: false,
                },
            },
            line,
        );
        let destination = self.allocate_register()?;
        self.emit(
            Instruction::FinishCall {
                destination,
                result: phera_bytecode::CallResult::Used,
            },
            line,
        );
        Ok(destination)
    }

    /// For `assert(condition)` with nothing else, the text `assert(condition)`
    /// that PHP uses as the failure message, and the name to pass it under:
    /// after `assert(assertion: $x)` the description must be named too.
    /// `None` for any other call.
    fn assertion_text(
        &self,
        name: &ast::Name,
        arguments: &ast::Arguments,
    ) -> Option<(Vec<u8>, Option<String>)> {
        let ast::Arguments::List(arguments) = arguments else {
            return None;
        };
        let [condition] = arguments.as_slice() else {
            return None;
        };
        if condition.spread || !name.text.eq_ignore_ascii_case("assert") {
            return None;
        }
        let description_name = match &condition.name {
            None => None,
            Some(named) if named.name == "assertion" => Some("description".to_string()),
            Some(_) => return None,
        };
        let formatted =
            super::assertion_text::format(self.source, condition.span, &condition.value);
        let written = formatted
            .as_deref()
            .or_else(|| self.source.get(condition.span.start..condition.span.end))?;
        let text = [b"assert(", written.trim_ascii(), b")"].concat();
        Some((text, description_name))
    }

    /// All call forms share named arguments, spreads and reference-capable sends.
    pub(crate) fn compile_arguments(
        &mut self,
        arguments: &ast::Arguments,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let ast::Arguments::List(arguments) = arguments else {
            let dst = self.allocate_register()?;
            self.emit(Instruction::FinishCallable { destination: dst }, line);
            return Ok(dst);
        };
        for argument in arguments {
            self.send_argument(argument, line)?;
        }
        let dst = self.allocate_register()?;
        self.emit(
            Instruction::FinishCall {
                destination: dst,
                result: phera_bytecode::CallResult::Used,
            },
            line,
        );
        Ok(dst)
    }

    /// Evaluates one argument and hands it to the call being set up.
    fn send_argument(&mut self, argument: &ast::Argument, line: usize) -> Result<(), CompileError> {
        let place = if !argument.spread && crate::helpers::reference_capable(&argument.value) {
            Some(self.readable_place(&argument.value)?)
        } else {
            None
        };
        let source = if let Some(place) = place {
            crate::ir::ArgumentSource::Place(place)
        } else if argument.spread && crate::helpers::reference_capable(&argument.value) {
            let place = self.place(&argument.value)?;
            let destination = self.allocate_register()?;
            self.emit(Instruction::Reference { destination, place }, line);
            crate::ir::ArgumentSource::Value(destination)
        } else {
            crate::ir::ArgumentSource::Value(self.compile_expression(&argument.value)?)
        };
        self.emit(
            Instruction::Send {
                argument: Argument {
                    source,
                    name: argument.name.as_ref().map(|name| name.name.clone()),
                    spread: argument.spread,
                },
            },
            line,
        );
        Ok(())
    }
}

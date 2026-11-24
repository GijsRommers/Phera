//! Callee-first call lowering and source-order argument sends.

use phera_ast as ast;
use phera_bytecode::{BuiltinFrame, Reg};

use crate::ir::{Argument, Constant, Instruction, Target};
use crate::policies::NameKind;
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Resolve the callee before sending arguments, since its signature governs references.
    pub(crate) fn compile_call(
        &mut self,
        callee: &ast::Expression,
        arguments: &ast::Arguments,
        line: usize,
        opening_line: usize,
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
        let name = match &callee.kind {
            ExpressionKind::Name(name) if callee.span == name.span => name,
            _ => {
                let callee = self.compile_expression(callee)?;
                self.emit(Instruction::StartDynamicCall { callee }, line);
                return self.compile_arguments(arguments, opening_line);
            }
        };
        let (resolved, fallback) = self.resolve(name, NameKind::Function);
        if fallback.is_none()
            && let Some(result) = self.compile_builtin_call(&resolved, arguments, line)?
        {
            return Ok(result);
        }
        if let Some(fallback) = &fallback
            && let Some(result) =
                self.compile_namespaced_builtin(&resolved, fallback, arguments, line)?
        {
            return Ok(result);
        }
        if let Some(fallback) = &fallback
            && let Some(result) =
                self.compile_namespaced_direct_native(&resolved, fallback, arguments, line)?
        {
            return Ok(result);
        }
        let assertion = fallback
            .as_deref()
            .unwrap_or(&resolved)
            .eq_ignore_ascii_case("assert");
        let call = Instruction::StartCall {
            name: resolved,
            fallback,
        };
        if assertion
            && self
                .native_functions
                .is_none_or(|native| (native.lookup)("assert").is_some())
            && let ast::Arguments::List(arguments) = arguments
        {
            return self.compile_assertion(call, arguments, line);
        }
        self.emit(call, line);
        self.compile_arguments(arguments, line)
    }

    fn compile_assertion(
        &mut self,
        call: Instruction,
        arguments: &[ast::Argument],
        line: usize,
    ) -> Result<Reg, CompileError> {
        let destination = self.allocate_register()?;
        self.emit(
            Instruction::BuiltinFrame {
                destination,
                operator: BuiltinFrame::AssertionsDisabled,
            },
            line,
        );
        let skip = self.emit(
            Instruction::Branch {
                value: destination,
                target: Target(0),
                when: true,
            },
            line,
        );
        self.emit(call, line);
        let temporary_objects = self.send_arguments(arguments, line)?;
        // PHP uses the written condition as the default failure message.
        if let Some((description, name)) = self.assertion_text(arguments) {
            let description = self.literal(Constant::Bytes(description), line)?;
            self.emit(
                Instruction::Send {
                    argument: Argument {
                        source: crate::ir::ArgumentSource::Value(description),
                        name,
                        spread: false,
                        of_call: false,
                    },
                },
                line,
            );
        }
        let source = self.finish_call(temporary_objects, line)?;
        self.emit(
            Instruction::Move {
                destination,
                source,
            },
            line,
        );
        self.patch(skip, self.position());
        Ok(destination)
    }

    /// For `assert(condition)` with nothing else, the text `assert(condition)`
    /// that PHP uses as the failure message, and the name to pass it under:
    /// after `assert(assertion: $x)` the description must be named too.
    /// The description is omitted when the argument shape does not supply
    /// one identifiable condition.
    fn assertion_text(&self, arguments: &[ast::Argument]) -> Option<(Vec<u8>, Option<String>)> {
        let [condition] = arguments else {
            return None;
        };
        if condition.spread {
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
        let temporary_objects = self.send_arguments(arguments, line)?;
        self.finish_call(temporary_objects, line)
    }

    fn send_arguments(
        &mut self,
        arguments: &[ast::Argument],
        line: usize,
    ) -> Result<Vec<Reg>, CompileError> {
        let mut temporary_objects = Vec::new();
        for argument in arguments {
            let mark = self.mark_temporaries();
            // An object created for the call (`f(new A)`) keeps its register
            // until `finish_call` clears it after the call; any other
            // argument's temporaries are free once it has been sent.
            match self.send_argument(argument, line)? {
                Some(register) => temporary_objects.push(register),
                None => self.release_temporaries(mark),
            }
        }
        Ok(temporary_objects)
    }

    pub(super) fn finish_call(
        &mut self,
        temporary_objects: Vec<Reg>,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let destination = self.allocate_register()?;
        self.emit(
            Instruction::FinishCall {
                destination,
                result: phera_bytecode::CallResult::Used,
            },
            line,
        );
        // The argument window owns a constructed object for the duration of the call.
        // Drop the compiler's copy before a following call can observe a weak reference.
        for register in temporary_objects {
            self.emit(Instruction::Clear { register }, line);
        }
        Ok(destination)
    }

    /// Evaluates one argument and hands it to the call being set up.
    fn send_argument(
        &mut self,
        argument: &ast::Argument,
        line: usize,
    ) -> Result<Option<Reg>, CompileError> {
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
        let temporary = match &source {
            crate::ir::ArgumentSource::Value(register)
                if matches!(argument.value.kind, ast::ExpressionKind::New { .. }) =>
            {
                Some(*register)
            }
            _ => None,
        };
        self.emit(
            Instruction::Send {
                argument: Argument {
                    source,
                    name: argument.name.as_ref().map(|name| name.name.clone()),
                    spread: argument.spread,
                    of_call: self.is_call_result(&argument.value),
                },
            },
            line,
        );
        Ok(temporary)
    }
}

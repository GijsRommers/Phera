//! Callee-first call lowering and source-order argument sends.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Argument, Instruction};
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
        if let ExpressionKind::Name(name) = &callee.kind
            && callee.span == name.span
        {
            let (name, fallback) = self.resolve(name, NameKind::Function);
            self.emit(Instruction::StartCall { name, fallback }, line);
        } else {
            let callee = self.compile_expression(callee)?;
            self.emit(Instruction::StartDynamicCall { callee }, line);
        }
        self.compile_arguments(arguments, line)
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
}

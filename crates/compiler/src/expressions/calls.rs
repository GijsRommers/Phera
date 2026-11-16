//! Callee-first call lowering and source-order argument sends.

use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Argument, Instruction};
use crate::policies::NameKind;
use crate::{CompileError, Compiler, writable};

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
        if let ExpressionKind::Name(name) = &callee.kind {
            let (name, fallback) = self.resolve(name, NameKind::Function);
            self.emit(Instruction::StartCall { name, fallback }, line);
        } else {
            let callee = self.compile_expression(callee)?;
            self.emit(Instruction::StartDynamicCall { callee }, line);
        }
        let ast::Arguments::List(arguments) = arguments else {
            let dst = self.allocate_register()?;
            self.emit(Instruction::FinishCallable { dst }, line);
            return Ok(dst);
        };
        for argument in arguments {
            let place = if !argument.spread && writable(&argument.value) {
                Some(self.place(&argument.value)?)
            } else {
                None
            };
            let value = if place.is_some() {
                Reg(0)
            } else {
                self.compile_expression(&argument.value)?
            };
            self.emit(
                Instruction::Send {
                    argument: Argument {
                        value,
                        place,
                        name: argument.name.as_ref().map(|name| name.name.clone()),
                        spread: argument.spread,
                    },
                },
                line,
            );
        }
        let dst = self.allocate_register()?;
        self.emit(Instruction::FinishCall { dst }, line);
        Ok(dst)
    }
}

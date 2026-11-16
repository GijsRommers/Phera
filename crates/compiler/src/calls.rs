//! Callee-first call lowering and source-order argument sends.
use phera_ast as ast;
use phera_bytecode::Reg;

use crate::ir::{Argument, Instruction};
use crate::{CompileError, Compiler, writable};
impl Compiler<'_> {
    pub(super) fn compile_call(
        &mut self,
        callee: &ast::Expression,
        arguments: &ast::Arguments,
        line: usize,
    ) -> Result<Reg, CompileError> {
        use ast::ExpressionKind as E;
        if let E::Name(name) = &callee.kind
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
            self.fun.dynamic_scope = true;
        }
        if let E::Name(name) = &callee.kind {
            let (name, fallback) = self.resolve(name, 1);
            self.emit(Instruction::StartCall { name, fallback }, line);
        } else {
            let callee = self.expr(callee)?;
            self.emit(Instruction::StartDynamicCall { callee }, line);
        }
        let ast::Arguments::List(args) = arguments else {
            let dst = self.reg()?;
            self.emit(Instruction::FinishCallable { dst }, line);
            return Ok(dst);
        };
        for arg in args {
            let place = if !arg.spread && writable(&arg.value) {
                Some(self.place(&arg.value)?)
            } else {
                None
            };
            let value = if place.is_some() {
                Reg(0)
            } else {
                self.expr(&arg.value)?
            };
            self.emit(
                Instruction::Send {
                    argument: Argument {
                        value,
                        place,
                        name: arg.name.as_ref().map(|name| name.name.clone()),
                        spread: arg.spread,
                    },
                },
                line,
            );
        }
        let dst = self.reg()?;
        self.emit(Instruction::FinishCall { dst }, line);
        Ok(dst)
    }
}

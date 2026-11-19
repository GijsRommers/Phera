//! Lexical closures keep their own frame, as in `function () use ($x) { return $x; }`.

use crate::statements::functions::{FunctionBody, FunctionSource};
use phera_ast as ast;
use phera_bytecode::{CaptureKind, ClosureBinding, Reg};

use crate::ir::Instruction;
use crate::policies::DeclarationTiming;
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// A closure body uses ordinary function lowering, including defaults and reference returns.
    pub(crate) fn compile_closure(
        &mut self,
        closure: &ast::Closure,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let captures = self.closure_captures(closure, line)?;
        let name = self.closure_name(line);
        let declaration = FunctionSource {
            span: closure.declaration_span,
            doc_comment: closure.doc_comment,
            attributes: &closure.attributes,
            name: &name,
            by_reference: closure.by_reference,
            is_generator: closure.is_generator,
            parameters: &closure.parameters,
            return_type: closure.return_type.as_ref(),
            body: FunctionBody::Statements(&closure.body),
        };
        let function = self.functions.len();
        self.compile_captured_function(&declaration, DeclarationTiming::Hidden, line, &captures)?;
        for capture in &closure.uses {
            let source = self.local(&capture.name)?;
            let kind = if capture.by_reference {
                CaptureKind::Reference
            } else {
                CaptureKind::Value
            };
            self.emit(Instruction::Capture { source, kind }, line);
        }
        self.emit_closure(function, closure.is_static, line)
    }

    pub(super) fn emit_closure(
        &mut self,
        function: usize,
        is_static: bool,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let destination = self.allocate_register()?;
        let binding = if is_static {
            ClosureBinding::Static
        } else {
            ClosureBinding::Bound
        };
        self.emit(
            Instruction::Closure {
                destination,
                function,
                binding,
            },
            line,
        );
        Ok(destination)
    }
}

impl Compiler<'_> {
    /// Lexical uses cannot shadow parameters or import PHP's automatic globals.
    fn closure_captures(
        &self,
        closure: &ast::Closure,
        line: usize,
    ) -> Result<Vec<String>, CompileError> {
        let mut captures = Vec::new();
        for capture in &closure.uses {
            if matches!(
                capture.name.as_str(),
                "GLOBALS"
                    | "_GET"
                    | "_POST"
                    | "_SERVER"
                    | "_COOKIE"
                    | "_FILES"
                    | "_ENV"
                    | "_SESSION"
                    | "_REQUEST"
            ) {
                let message = "Cannot use auto-global as lexical variable";
                return Err(self.error(message, line));
            }
            if capture.name == "this" {
                return Err(self.error("Cannot use $this as lexical variable", line));
            }
            if captures.contains(&capture.name) {
                let message = format!("Cannot use variable ${} twice", capture.name);
                return Err(self.error(message, line));
            }
            if closure
                .parameters
                .iter()
                .any(|parameter| parameter.name == capture.name)
            {
                let message = format!(
                    "Cannot use lexical variable ${} as a parameter name",
                    capture.name
                );
                return Err(self.error(message, line));
            }
            captures.push(capture.name.clone());
        }
        Ok(captures)
    }

    /// PHP 8.5 names nested closures after their enclosing function for traces and diagnostics.
    pub(super) fn closure_name(&self, line: usize) -> String {
        let parent = if self.function.name == "{main}" {
            self.file.to_owned()
        } else if self.function.name.starts_with("{closure:") {
            self.function.name.clone()
        } else {
            format!("{}()", self.function.name)
        };
        format!("{{closure:{parent}:{line}}}")
    }
}

//! Lexical closures keep their own frame, as in `function () use ($x) { return $x; }`.

use phera_ast as ast;
use phera_bytecode::{CaptureKind, ClosureBinding, Reg};
use phera_source::Span;

use crate::ir::Instruction;
use crate::policies::DeclarationTiming;
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// A closure body uses ordinary function lowering, including defaults and reference returns.
    pub(crate) fn compile_closure(
        &mut self,
        closure: &ast::Closure,
        span: Span,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let captures = self.closure_captures(closure, line)?;
        let declaration = ast::FunctionDeclaration {
            attributes: closure.attributes.clone(),
            name: ast::Identifier {
                name: self.closure_name(line),
                span,
            },
            by_reference: closure.by_reference,
            parameters: closure.parameters.clone(),
            return_type: closure.return_type.clone(),
            body: closure.body.clone(),
        };
        let function = self.functions.len();
        self.compile_captured_function(&declaration, DeclarationTiming::Hoisted, line, &captures)?;
        self.hoisted.retain(|candidate| *candidate != function);
        for capture in &closure.uses {
            let source = self.local(&capture.name)?;
            let kind = if capture.by_reference {
                CaptureKind::Reference
            } else {
                CaptureKind::Value
            };
            self.emit(Instruction::Capture { source, kind }, line);
        }
        let destination = self.allocate_register()?;
        let binding = if closure.is_static {
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
    fn closure_name(&self, line: usize) -> String {
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

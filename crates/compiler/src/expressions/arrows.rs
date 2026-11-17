//! Arrow functions capture referenced outer locals by value: `$f = fn($x) => $x + $offset;`.

use phera_ast as ast;
use phera_bytecode::{CaptureKind, Reg};
use phera_source::Span;

use crate::ir::Instruction;
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Lower once before inspecting locals, so nested arrows propagate uses without an AST rewalk.
    pub(crate) fn compile_arrow(
        &mut self,
        arrow: &ast::ArrowFunction,
        span: Span,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let closure = ast::Closure {
            attributes: arrow.attributes.clone(),
            is_static: arrow.is_static,
            by_reference: arrow.by_reference,
            parameters: arrow.parameters.clone(),
            uses: Vec::new(),
            return_type: arrow.return_type.clone(),
            body: vec![ast::Statement {
                kind: ast::StatementKind::Return(Some(arrow.body.clone())),
                span,
            }],
        };
        let function = self.functions.len();
        let destination = self.compile_closure(&closure, span, line)?;
        let creation = self
            .function
            .instructions
            .pop()
            .ok_or_else(|| self.error("Missing arrow creation", line))?;
        self.function.lines.pop();
        let body = &mut self.functions[function];
        let captures = body.locals[body.parameters.len()..].to_vec();
        body.captures = captures.len();
        for name in captures {
            let source = self.local(&name)?;
            self.emit(
                Instruction::Capture {
                    source,
                    kind: CaptureKind::Quiet,
                },
                line,
            );
        }
        self.emit(creation, line);
        Ok(destination)
    }
}

//! Arrow functions capture referenced outer locals by value: `$f = fn($x) => $x + $offset;`.

use crate::policies::DeclarationTiming;
use crate::statements::functions::{FunctionBody, FunctionSource};
use phera_ast as ast;
use phera_bytecode::{CaptureKind, Reg};

use crate::ir::Instruction;
use crate::{CompileError, Compiler};

impl Compiler<'_> {
    /// Lower once before inspecting locals, so nested arrows propagate uses without an AST rewalk.
    pub(crate) fn compile_arrow(
        &mut self,
        arrow: &ast::ArrowFunction,
        line: usize,
    ) -> Result<Reg, CompileError> {
        let name = self.closure_name(line);
        let declaration = FunctionSource {
            span: arrow.declaration_span,
            doc_comment: arrow.doc_comment,
            attributes: &arrow.attributes,
            name: &name,
            by_reference: arrow.by_reference,
            is_generator: arrow.is_generator,
            parameters: &arrow.parameters,
            return_type: arrow.return_type.as_ref(),
            body: FunctionBody::Arrow(&arrow.body),
        };
        let function = self.functions.len();
        self.compile_captured_function(&declaration, DeclarationTiming::Hidden, line, &[])?;
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
        self.emit_closure(function, arrow.is_static, line)
    }
}

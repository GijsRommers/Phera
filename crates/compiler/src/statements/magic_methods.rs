//! Validate `function __get(string $name): mixed` before compiling the body.

use phera_ast::Method;
use phera_bytecode::DeclarationWarning;

use crate::{CompileError, Compiler};

impl Compiler<'_> {
    pub(super) fn validate_magic_method(
        &mut self,
        class: &str,
        method: &Method,
    ) -> Result<(), CompileError> {
        let line = self.lines.line(method.name.span.start);
        let mut warning = None;
        let result = phera_parser::magic_methods::validate(
            class,
            method,
            |ty| self.declaration_type(ty),
            |message| warning = Some(message),
        );
        if let Some(message) = warning {
            self.declaration_warnings.push(DeclarationWarning {
                message,
                line: line as u32,
            });
        }
        result.map_err(|message| CompileError {
            message,
            file: self.file.into(),
            line,
            parse: false,
            scanner_diagnostics: Vec::new(),
        })
    }
}

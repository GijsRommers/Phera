//! A bad declaration in `include 'bad.php'` is fatal to the request.
//! In contrast, PHP lets `try { eval($source); } catch (ParseError $e) {}`
//! catch a syntax error in dynamically compiled source.

use phera_compiler::CompileError;
use phera_extension_api::{CompileDiagnosticSite, Context, NativeError, Unwind};

use crate::{Vm, diagnostics::SourceLocation, error_traces::DeclarationFailure};

impl Vm {
    /// An entry script never starts if its compilation failed.
    pub fn report_entry_compile_failure(&mut self, error: CompileError) {
        for diagnostic in &error.scanner_diagnostics {
            self.compile_diagnostic(diagnostic, CompileDiagnosticSite::Source(&error.file));
        }
        let location = SourceLocation {
            file: error.file,
            line: error.line,
        };
        if error.parse {
            let message = if error.message.starts_with("unexpected token ") {
                format!("syntax error, {}", error.message)
            } else {
                error.message
            };
            self.report_diagnostic(4, "Parse error", message.into(), location);
        } else {
            self.report_fatal(64, error.message, location);
        }
    }

    pub(crate) async fn dynamic_compile_failure(&mut self, error: CompileError) -> NativeError {
        for diagnostic in &error.scanner_diagnostics {
            self.compile_diagnostic(diagnostic, CompileDiagnosticSite::Source(&error.file));
        }
        if let Err(handler_error) = self.drain_error_handlers().await {
            return handler_error;
        }
        if error.parse {
            let previous = self.diagnostic_location.replace(SourceLocation {
                file: error.file,
                line: error.line,
            });
            let error = self.materialize_error(NativeError::throw("ParseError", error.message));
            self.diagnostic_location = previous;
            error
        } else {
            NativeError::Unwind(Unwind(Box::new(DeclarationFailure {
                message: error.message,
                file: error.file,
                line: error.line,
            })))
        }
    }
}

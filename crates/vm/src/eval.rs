//! `$result = eval('$count++; return $count;');` runs a new unit in the caller's scope.

use std::sync::Arc;

use phera_bytecode::Reg;
use phera_extension_api::{CompileDiagnosticSite, Context, NativeError};

use crate::{Vm, diagnostics::SourceLocation};

impl Vm {
    /// Evaluated units belong to this request and use the same frame protocol as includes.
    pub(crate) async fn eval(&mut self, destination: Reg, source: Reg) -> Result<(), NativeError> {
        // The code keeps its bytes: a string literal inside it may hold any.
        let source = self
            .stringify_value(&self.get(source))
            .await?
            .as_bytes()
            .to_vec();
        if source.is_empty() {
            self.set(destination, phera_value::Value::Bool(false));
            return Ok(());
        }
        let location = self.location();
        let file = format!("{}({}) : eval()'d code", location.file, location.line);
        // Ordinary files count occurrences from zero. Give each evaluated
        // unit its own range even when two executions share the source line.
        let anonymous_seed = u64::from(self.code.next_unit_index().0) << 32;
        let compiled = tokio::task::spawn_blocking(move || {
            phera_compiler::compile_eval_with_seed(&source, &file, anonymous_seed)
        })
        .await
        .map_err(|error| NativeError::error(format!("Eval compiler failed: {error}")))?;
        if let Err(error) = &compiled {
            for diagnostic in &error.scanner_diagnostics {
                self.compile_diagnostic(diagnostic, CompileDiagnosticSite::Source(&error.file));
            }
            self.drain_error_handlers().await?;
        }
        match compiled {
            Ok(unit) => self.enter_scoped_unit(Arc::new(unit), Some(destination)),
            Err(error) if error.parse => {
                let previous = self.diagnostic_location.replace(SourceLocation {
                    file: error.file,
                    line: error.line,
                });
                let error = self.materialize_error(NativeError::throw("ParseError", error.message));
                self.diagnostic_location = previous;
                Err(error)
            }
            Err(error) => Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
                crate::error_traces::DeclarationFailure {
                    message: error.message,
                    file: error.file,
                    line: error.line,
                },
            )))),
        }
    }
}

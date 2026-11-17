//! PHP warns before execution for declarations such as `private function __invoke() {}`.

use phera_bytecode::CompiledUnit;

use crate::Vm;
use crate::diagnostics::SourceLocation;

impl Vm {
    /// Declaration locations survive caching so include warnings reach the active PHP handler.
    pub(crate) fn publish_declaration_warnings(&mut self, unit: &CompiledUnit) {
        for warning in &unit.declaration_warnings {
            let location = SourceLocation {
                file: unit.file.clone(),
                line: warning.line as usize,
            };
            let previous = self.diagnostic_location.replace(location);
            self.diagnostic("Warning", warning.message.clone());
            self.diagnostic_location = previous;
        }
    }
}

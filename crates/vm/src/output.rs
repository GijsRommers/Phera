//! PHP output instruction handlers, separate from dispatch.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::to_php_string;

use crate::Vm;

impl Vm {
    /// Implements PHP Echo semantics outside the dispatch loop.
    pub(crate) fn echo_value(&mut self, src: Reg) -> Result<(), NativeError> {
        let mut diagnostics = std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
        let text = to_php_string(&self.operand(src), &mut diagnostics)?;
        self.diagnostics_flush(diagnostics);
        self.out(text.as_bytes());
        Ok(())
    }
}

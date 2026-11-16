//! PHP errors instruction handlers, separate from dispatch.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;

use crate::Vm;

impl Vm {
    /// Implements PHP Raise semantics outside the dispatch loop.
    pub(crate) fn raise_match_error(&mut self, detail: Reg) -> Result<(), NativeError> {
        let value = self.get(detail);
        let description = crate::error_values::describe(&value);
        Err(NativeError::throw(
            "UnhandledMatchError",
            format!("Unhandled match case {description}"),
        ))
    }
}

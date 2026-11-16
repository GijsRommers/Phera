//! PHP errors instruction handlers, separate from dispatch.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::{Value, to_php_string};

use crate::Vm;

impl Vm {
    /// Implements PHP Raise semantics outside the dispatch loop.
    pub(crate) fn raise_match_error(&mut self, detail: Reg) -> Result<(), NativeError> {
        let value = self.get(detail);
        let description = match &value {
            Value::String(_) => "of type string".to_owned(),
            Value::Array(_) => "of type array".to_owned(),
            _ => {
                let mut diagnostics =
                    std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
                to_php_string(&value, &mut diagnostics)?.to_string_lossy()
            }
        };
        Err(NativeError::throw(
            "UnhandledMatchError",
            format!("Unhandled match case {description}"),
        ))
    }
}

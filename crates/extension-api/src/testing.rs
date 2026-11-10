//! A stand-in [`Context`] for unit-testing native functions without a VM.

use phera_value::{CollectDiagnostics, Diagnostics, Value};

use crate::{Context, LocalFuture, NativeError, NativeResult};

/// Records output and diagnostics. Calling back into PHP is not possible
/// without a VM, so [`Context::call`] fails.
#[derive(Default)]
pub struct TestContext {
    pub output: Vec<u8>,
    pub diagnostics: CollectDiagnostics,
    pub strict_types: bool,
}

impl Context for TestContext {
    fn write(&mut self, bytes: &[u8]) {
        self.output.extend_from_slice(bytes);
    }

    fn diagnostics(&mut self) -> &mut dyn Diagnostics {
        &mut self.diagnostics
    }

    fn strict_types(&self) -> bool {
        self.strict_types
    }

    fn call<'a>(
        &'a mut self,
        _callable: &'a Value,
        _arguments: Vec<Value>,
    ) -> LocalFuture<'a, NativeResult> {
        Box::pin(async { Err(NativeError::error("TestContext cannot call PHP callables")) })
    }
}

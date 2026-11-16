//! A stand-in [`Context`] for unit-testing native functions without a VM.

use phera_value::{CollectDiagnostics, Diagnostics, Value};

use crate::{Context, FunctionEntry, LocalFuture, NativeError, NativeFn, NativeResult, Registry};

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

/// Calls a sync native function the way the VM does: arguments are prepared
/// against the signature first, then the body runs.
///
/// Fails for a name that is not registered and for async functions, which
/// need a runtime to await them.
pub fn call(
    registry: &Registry,
    context: &mut TestContext,
    name: &str,
    mut arguments: Vec<Value>,
) -> NativeResult {
    let entry: &FunctionEntry = registry
        .get_function(name)
        .ok_or_else(|| NativeError::error(format!("Call to undefined function {name}()")))?;
    crate::prepare_arguments(entry, &mut arguments, context)?;
    match entry.handler {
        NativeFn::Sync(function) => function(context, &arguments),
        NativeFn::Async(_) => Err(NativeError::error(format!(
            "{name}() is async and cannot be called from testing::call"
        ))),
    }
}

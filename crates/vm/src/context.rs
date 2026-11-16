//! Native extensions use Context; callbacks are bounded independently of PHP frames.

use phera_extension_api::{Context, LocalFuture, NativeResult, RequestState};
use phera_value::{CollectDiagnostics, Diagnostics, Value, to_php_string};

use crate::Vm;

impl Diagnostics for Vm {
    fn warning(&mut self, message: String) {
        self.diagnostic("Warning", message)
    }

    fn notice(&mut self, message: String) {
        self.diagnostic("Notice", message)
    }

    fn deprecated(&mut self, message: String) {
        self.diagnostic("Deprecated", message)
    }
}

impl Context for Vm {
    fn write(&mut self, bytes: &[u8]) {
        self.out(bytes)
    }

    fn diagnostics(&mut self) -> &mut dyn Diagnostics {
        self
    }

    fn state(&mut self) -> &mut RequestState {
        &mut self.state
    }

    fn strict_types(&self) -> bool {
        self.current().is_some_and(|f| f.function().strict_types)
    }

    /// Native callbacks await the heap interpreter with a separate bound on Rust nesting.
    fn call<'a>(
        &'a mut self,
        callable: &'a Value,
        arguments: Vec<Value>,
    ) -> LocalFuture<'a, NativeResult> {
        Box::pin(async move {
            self.call_stack.enter_callback()?;
            let result = async {
                let mut diagnostics = CollectDiagnostics::default();
                let name = to_php_string(callable, &mut diagnostics)?.to_string_lossy();
                let stop = self.call_stack.depth();
                if let Some(function) = self.function_table.get(&name.to_ascii_lowercase()).cloned()
                {
                    self.push_call(function.unit, function.index, None, arguments)?;
                    self.interpret(stop).await
                } else {
                    Box::pin(self.native(&name, arguments)).await
                }
            }
            .await;
            self.call_stack.leave_callback();
            result
        })
    }
}

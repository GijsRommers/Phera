//! Test support: a stand-in [`Context`] for calling native functions without
//! a VM, [`oracle`] for comparing them against real PHP, [`scripts`] for
//! comparing whole PHP files run by PHP and by the `phera` executable, and
//! [`serve`] for sending requests to `phera serve`.

mod classes;
mod exceptions;
pub mod oracle;
pub mod scripts;
pub mod serve;

pub use classes::{call_method, instantiate};
pub use exceptions::materialize_exception;

use std::rc::Rc;

use phera_value::{CollectDiagnostics, Diagnostics, Object, PhpArray, Value};

use crate::{
    CompileDiagnostic, CompileDiagnosticKind, CompileDiagnosticSite, Context, FunctionEntry,
    LocalFuture, NativeError, NativeFn, NativeResult, Registry, RequestState,
};

/// Records output and diagnostics. Calling back into PHP is not possible
/// without a VM, so [`Context::call`] fails.
#[derive(Default)]
pub struct TestContext {
    pub output: Vec<u8>,
    pub diagnostics: CollectDiagnostics,
    pub strict_types: bool,
    pub state: RequestState,
    pub caller: Option<(String, usize)>,
}

impl Context for TestContext {
    fn write(&mut self, bytes: &[u8]) {
        self.output.extend_from_slice(bytes);
    }

    fn diagnostics(&mut self) -> &mut dyn Diagnostics {
        &mut self.diagnostics
    }

    fn compile_diagnostic(
        &mut self,
        diagnostic: &CompileDiagnostic,
        site: CompileDiagnosticSite<'_>,
    ) {
        match diagnostic.kind {
            CompileDiagnosticKind::Warning => {
                let (file, line) = match site {
                    CompileDiagnosticSite::Caller => self
                        .caller
                        .as_ref()
                        .map_or(("Unknown", diagnostic.line), |(file, line)| {
                            (file.as_str(), *line)
                        }),
                    CompileDiagnosticSite::Source(file) => (file, diagnostic.line),
                };
                self.write(diagnostic.render(file, line).as_bytes());
            }
            CompileDiagnosticKind::Deprecated => self.deprecated(diagnostic.message.clone()),
        }
    }

    fn state(&mut self) -> &mut RequestState {
        &mut self.state
    }

    fn strict_types(&self) -> bool {
        self.strict_types
    }

    fn create_object(&mut self, class: &str, properties: PhpArray) -> NativeResult {
        let object = Object::new(1, Rc::new(classes::NamedClass(class.to_string())));
        object.update(|table| *table = properties);
        Ok(Value::Object(Rc::new(object)))
    }

    fn call<'a>(
        &'a mut self,
        _callable: &'a Value,
        _arguments: Vec<Value>,
    ) -> LocalFuture<'a, NativeResult> {
        Box::pin(async { Err(NativeError::error("TestContext cannot call PHP callables")) })
    }
}

/// Prepares native arguments and runs a handler that can complete immediately.
/// A handler waiting on the runtime needs an integration test instead.
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
        NativeFn::Async(function) => {
            let mut future = function(context, arguments);
            let mut task = std::task::Context::from_waker(std::task::Waker::noop());
            match future.as_mut().poll(&mut task) {
                std::task::Poll::Ready(result) => result,
                std::task::Poll::Pending => Err(NativeError::error(format!(
                    "{name}() is waiting for a runtime"
                ))),
            }
        }
    }
}

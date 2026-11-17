//! Native extensions use Context; callbacks are bounded independently of PHP frames.

use phera_extension_api::{Context, LocalFuture, NativeResult, RequestState};
use phera_value::{Diagnostics, Value};

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
                let callee = self.resolve_callable(callable)?;
                let stop = self.call_stack.depth();
                match &callee.target {
                    crate::call_state::CallTarget::User(function) => {
                        self.push_call(function.unit.clone(), function.index, None, arguments)?;
                        self.interpret_frames(stop).await
                    }
                    crate::call_state::CallTarget::Method { method, context } => {
                        match &method.body {
                            crate::classes::MethodBody::User(function) => {
                                self.push_call(
                                    function.unit.clone(),
                                    function.index,
                                    None,
                                    arguments,
                                )?;
                                if let Some(frame) = self.call_stack.current_mut() {
                                    frame.method = Some(Box::new(context.clone()));
                                }
                                self.interpret_frames(stop).await
                            }
                            crate::classes::MethodBody::Native(entry) => {
                                let mut arguments = arguments;
                                phera_extension_api::prepare_call(
                                    &callee.name,
                                    entry.params,
                                    &mut arguments,
                                    self,
                                )?;
                                match entry.handler {
                                    phera_extension_api::NativeMethod::Sync(handler) => {
                                        handler(self, context.receiver.as_ref(), &arguments)
                                    }
                                    phera_extension_api::NativeMethod::Async(handler) => {
                                        handler(self, context.receiver.clone(), arguments).await
                                    }
                                    phera_extension_api::NativeMethod::Abstract => {
                                        Err(phera_extension_api::NativeError::error(
                                            "Cannot call abstract method",
                                        ))
                                    }
                                }
                            }
                        }
                    }
                    _ => Box::pin(self.native(&callee.name, arguments)).await,
                }
            }
            .await;
            self.call_stack.leave_callback();
            result
        })
    }
}

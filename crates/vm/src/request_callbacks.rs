//! `register_shutdown_function(fn () => print "done")` runs after the
//! main script. A shutdown callback can append another callback to the queue.

use std::collections::VecDeque;

use phera_extension_api::{NativeError, NativeResult, RequestCallback};
use phera_value::Value;

use crate::{Vm, calls::call_state::Callee};

#[derive(Default)]
pub(crate) struct RequestCallbacks {
    pub(crate) headers: Option<Callee>,
    shutdown: VecDeque<(Callee, Vec<Value>)>,
}

impl Vm {
    pub(crate) async fn register_callback(
        &mut self,
        phase: RequestCallback,
        arguments: Vec<Value>,
    ) -> NativeResult {
        let mut arguments = arguments.into_iter();
        let Some(callable) = arguments.next() else {
            return Err(internal_error!("request callback argument missing"));
        };
        let function = match phase {
            RequestCallback::Headers => "header_register_callback",
            RequestCallback::Shutdown => "register_shutdown_function",
        };
        let callee = self.request_callable(function, &callable).await?;
        match phase {
            RequestCallback::Headers => {
                if self.output_writer.origin.is_none() {
                    self.request_callbacks.headers = Some(callee);
                }
                Ok(Value::Bool(true))
            }
            RequestCallback::Shutdown => {
                self.request_callbacks
                    .shutdown
                    .push_back((callee, arguments.collect()));
                Ok(Value::Null)
            }
        }
    }

    pub(crate) async fn request_callable(
        &mut self,
        function: &str,
        callable: &Value,
    ) -> Result<Callee, NativeError> {
        self.autoload_callable(callable).await?;
        self.resolve_callable(callable).map_err(|error| {
            let detail = match (callable, error) {
                (Value::String(name), NativeError::Throw { message, .. })
                    if message.starts_with("Call to undefined function ") =>
                {
                    format!(
                        "function \"{}\" not found or invalid function name",
                        name.to_string_lossy()
                    )
                }
                (Value::Array(_), NativeError::Throw { message, .. })
                    if message == "Array callback must have exactly two elements" =>
                {
                    "array callback must have exactly two members".into()
                }
                (Value::Array(_), NativeError::Throw { message, .. }) => message,
                _ => "no array or string given".into(),
            };
            NativeError::type_error(format!(
                "{function}(): Argument #1 ($callback) must be a valid callback, {detail}"
            ))
        })
    }

    pub(crate) async fn run_shutdown_callbacks(&mut self) {
        while let Some((callback, arguments)) = self.request_callbacks.shutdown.pop_front() {
            if let Err(error) = self.invoke_resolved_callable(callback, arguments).await {
                self.unwind(error);
                break;
            }
        }
        self.request_callbacks.shutdown.clear();
    }
}

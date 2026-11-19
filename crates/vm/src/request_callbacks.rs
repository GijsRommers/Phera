//! `register_shutdown_function(fn () => print "done")` runs after the
//! main script. A shutdown callback can append another callback to the queue.

use std::collections::VecDeque;

use phera_extension_api::{NativeError, NativeResult, RequestCallback};
use phera_value::Value;

use crate::{Vm, calls::call_state::Callee};

#[derive(Default)]
pub(crate) struct RequestCallbacks {
    pub(crate) headers: Option<Callee>,
    pub(crate) finalizer: Option<phera_extension_api::AsyncFn>,
    shutdown: VecDeque<Shutdown>,
}

struct Shutdown {
    key: Option<&'static str>,
    callback: Callee,
    arguments: Vec<Value>,
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
                self.request_callbacks.shutdown.push_back(Shutdown {
                    key: None,
                    callback: callee,
                    arguments: arguments.collect(),
                });
                Ok(Value::Null)
            }
        }
    }

    pub(crate) async fn register_named_shutdown(
        &mut self,
        key: &'static str,
        callback: Option<&'static str>,
    ) -> NativeResult {
        if let Some(callback) = callback {
            let callback = self.request_callable(key, &Value::from(callback)).await?;
            if let Some(entry) = self
                .request_callbacks
                .shutdown
                .iter_mut()
                .find(|entry| entry.key == Some(key))
            {
                entry.callback = callback;
            } else {
                self.request_callbacks.shutdown.push_back(Shutdown {
                    key: Some(key),
                    callback,
                    arguments: Vec::new(),
                });
            }
        } else {
            self.request_callbacks
                .shutdown
                .retain(|entry| entry.key != Some(key));
        }
        Ok(Value::Null)
    }

    pub(crate) async fn request_callable(
        &mut self,
        function: &str,
        callable: &Value,
    ) -> Result<Callee, NativeError> {
        self.autoload_callable(callable).await?;
        self.resolve_native_callback(function, 1, "callback", callable)
    }

    pub(crate) fn resolve_native_callback(
        &mut self,
        function: &str,
        number: usize,
        parameter: &str,
        callable: &Value,
    ) -> Result<Callee, NativeError> {
        self.resolve_callable(callable).map_err(|error| {
            self.invalid_callback(function, number, parameter, callable, error, false)
        })
    }

    pub(crate) async fn run_shutdown_callbacks(&mut self) {
        while let Some(Shutdown {
            callback,
            arguments,
            ..
        }) = self.request_callbacks.shutdown.pop_front()
        {
            if let Err(error) = self.invoke_resolved_callable(callback, arguments).await
                && !self.report_uncaught(error).await
            {
                break;
            }
        }
        self.request_callbacks.shutdown.clear();
    }
}

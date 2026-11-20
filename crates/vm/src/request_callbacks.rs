//! `register_shutdown_function(fn () => print "done")` runs after the
//! main script. A shutdown callback can append another callback to the queue.

use std::collections::VecDeque;

use phera_extension_api::{AsyncFn, NativeError, NativeResult, RequestCallback};
use phera_value::Value;

use crate::{Vm, calls::call_state::Callee};

#[derive(Default)]
pub(crate) struct RequestCallbacks {
    pub(crate) output: OutputCallbacks,
    shutdown: VecDeque<Shutdown>,
}

/// The output dispatcher already has to distinguish between a request with
/// `header_register_callback()` and one with no work to run around output.
#[derive(Default)]
pub(crate) enum OutputCallbacks {
    #[default]
    None,
    Headers(Callee),
    AfterOutput(AsyncFn),
    Both(Callee, AsyncFn),
}

impl OutputCallbacks {
    pub(crate) fn has_headers(&self) -> bool {
        matches!(self, Self::Headers(_) | Self::Both(..))
    }

    pub(crate) fn set_finalizer(&mut self, finalizer: AsyncFn) {
        *self = match std::mem::take(self) {
            Self::Headers(headers) | Self::Both(headers, _) => Self::Both(headers, finalizer),
            Self::None | Self::AfterOutput(_) => Self::AfterOutput(finalizer),
        };
    }

    fn set_headers(&mut self, headers: Callee) {
        *self = match std::mem::take(self) {
            Self::AfterOutput(finalizer) | Self::Both(_, finalizer) => {
                Self::Both(headers, finalizer)
            }
            Self::None | Self::Headers(_) => Self::Headers(headers),
        };
    }

    pub(crate) fn into_callbacks(self) -> (Option<Callee>, Option<AsyncFn>) {
        match self {
            Self::None => (None, None),
            Self::Headers(headers) => (Some(headers), None),
            Self::AfterOutput(finalizer) => (None, Some(finalizer)),
            Self::Both(headers, finalizer) => (Some(headers), Some(finalizer)),
        }
    }
}

#[derive(Default)]
pub(crate) struct ResourceFinalizers(Vec<(&'static str, AsyncFn)>);

impl ResourceFinalizers {
    pub(crate) fn register(&mut self, key: &'static str, callback: AsyncFn) {
        if let Some((_, registered)) = self.0.iter_mut().find(|(name, _)| *name == key) {
            *registered = callback;
        } else {
            self.0.push((key, callback));
        }
    }
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
                    self.request_callbacks.output.set_headers(callee);
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

    pub(crate) async fn finish_resource_finalizers(&mut self) {
        if let Some(finalizers) = self.state_mut().take::<ResourceFinalizers>() {
            for (_, finalize) in finalizers.0 {
                if let Err(error) = finalize(self, Vec::new()).await {
                    self.report_uncaught(error).await;
                }
            }
        }
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

impl crate::snapshot::ForkCopy for Shutdown {
    fn fork_copy(&self, fork: &mut phera_value::Fork<'_>) -> Self {
        Shutdown {
            key: self.key,
            callback: self.callback.fork_copy(fork),
            arguments: self.arguments.fork_copy(fork),
        }
    }
}

/// Shutdown functions registered before the snapshot run at the end of
/// every fork's request, on that fork's copies.
impl crate::snapshot::ForkCopy for RequestCallbacks {
    fn fork_copy(&self, fork: &mut phera_value::Fork<'_>) -> Self {
        let output = match &self.output {
            OutputCallbacks::None => OutputCallbacks::None,
            OutputCallbacks::Headers(headers) => OutputCallbacks::Headers(headers.fork_copy(fork)),
            OutputCallbacks::AfterOutput(finalizer) => OutputCallbacks::AfterOutput(*finalizer),
            OutputCallbacks::Both(headers, finalizer) => {
                OutputCallbacks::Both(headers.fork_copy(fork), *finalizer)
            }
        };
        RequestCallbacks {
            output,
            shutdown: self.shutdown.fork_copy(fork),
        }
    }
}

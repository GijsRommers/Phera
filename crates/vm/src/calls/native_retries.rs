//! A native call whose synchronous part missed a class runs again once the
//! class is loaded.
//!
//! ```php
//! class P1 { public int $s = Strategy::IMPLICIT; }   // Strategy comes from the autoloader
//! outer((new ReflectionClass('P1'))->newInstanceWithoutConstructor(), 'ok');
//! ```
//!
//! Evaluating the default of `P1::$s` needs `Strategy`, and that step cannot
//! run the autoloader, so it unwinds with a [`ClassMiss`]. For an instruction
//! the interpreter loads the class and runs the instruction again
//! (`Vm::resume_class_fetch`). A native call cannot be run again from its
//! `Call`: it has taken its prepared call and its arguments by then, and the
//! next prepared call on the stack is the caller's (`outer` above, which then
//! ran with no arguments). So the native call boundaries load the class here
//! and call the native again with the same arguments: the boundaries that
//! can wait do it themselves ([`Vm::load_missed_class`]), and the synchronous
//! ones in the batch loop put the call back as it was, so that the
//! interpreter's own retry of the `Call` finds it ([`Vm::restore_native_call`]).
//!
//! None of this costs a call that does not miss: the miss is seen in the
//! error branch the call has anyway, and everything after it is in the cold
//! functions below. The arguments for the second call are where they are
//! already: borrowed by a synchronous handler, in the copy an async one
//! keeps for the native frame of a failure, or in the trace of a native that
//! calls back into PHP (for a native with no by-reference or sensitive
//! parameter, as the trace keeps a copy of each value and masks sensitive
//! ones). A native called with none of these (as a callback of another
//! native) reports the missing class as an error instead.

use std::rc::Rc;

use phera_extension_api::{AsyncFn, LocalFuture, NativeError, NativeMethod, NativeResult, SyncFn};
use phera_value::{Object, PhpString, Value};

use crate::Vm;
use crate::autoload::ClassMiss;
use crate::calls::callback_binding::ArgumentBinding;
use crate::calls::native_frames::NativeFrame;
use crate::calls::prepared_call::{CallDetails, PreparedCall};

/// How many missing classes one native call may load before the miss is
/// reported as an error, so that a class that never appears cannot make a
/// call loop.
const MOST_CLASS_MISSES: usize = 64;

impl Vm {
    /// For a native call that failed with `error`: when it is a class miss
    /// and the call can run again (`retry`), loads the class and gives
    /// `Ok`, for the caller to call the native again. Otherwise the error
    /// to report: PHP's own for a class that stays missing.
    pub(crate) async fn load_missed_class(
        &mut self,
        error: NativeError,
        attempts: &mut usize,
        retry: bool,
    ) -> Result<(), NativeError> {
        let miss = ClassMiss::take(error)?;
        if !retry || *attempts >= MOST_CLASS_MISSES {
            return Err(miss.error());
        }
        *attempts += 1;
        self.resolve_class_miss(miss).await
    }

    /// Puts a synchronous native call back as it was before it took its
    /// prepared call and arguments: the call record, its details, what the
    /// window keeps below its first register (`$this`, a closure), and the
    /// arguments. For a class miss from the native, after which the
    /// interpreter runs the `Call` again.
    #[cold]
    pub(crate) fn restore_native_call(
        &mut self,
        (call, details): (PreparedCall, Option<CallDetails>),
        prefix: [Option<Value>; 2],
        arguments: &[Value],
    ) {
        let base = call.base();
        self.registers.grow_to(base + arguments.len().max(1));
        for (slot, value) in (call.start()..base).zip(prefix) {
            if let Some(value) = value {
                self.registers.put(slot, value);
            }
        }
        for (offset, argument) in arguments.iter().enumerate() {
            self.registers.put(base + offset, argument.clone());
        }
        self.call_stack.push_call(call, details);
    }

    /// A synchronous native function's handler, after it failed with a
    /// class miss (`failed`): loads the class and calls it again.
    #[cold]
    pub(crate) fn retry_sync_native<'a>(
        &'a mut self,
        name: &'static str,
        handler: SyncFn,
        arguments: &'a [Value],
        binding: ArgumentBinding,
        failed: NativeResult,
    ) -> LocalFuture<'a, NativeResult> {
        Box::pin(async move {
            let mut attempts = 0;
            let mut failed = failed;
            loop {
                let Err(error) = failed else {
                    return failed;
                };
                self.load_missed_class(error, &mut attempts, true).await?;
                failed = handler(self, arguments);
                if let Err(error) = &failed
                    && !ClassMiss::is(error)
                    && binding.has_native_frame()
                {
                    self.record_native_failure(error, NativeFrame::Function(name), arguments);
                }
                if !failed.as_ref().is_err_and(ClassMiss::is) {
                    return failed;
                }
            }
        })
    }

    /// An async native function's handler, after it failed with a class
    /// miss: loads the class and calls it again with `arguments`.
    #[cold]
    pub(crate) fn retry_async_native(
        &mut self,
        name: &'static str,
        handler: AsyncFn,
        arguments: Vec<Value>,
        failed: NativeResult,
    ) -> LocalFuture<'_, NativeResult> {
        Box::pin(async move {
            let mut attempts = 0;
            let mut failed = failed;
            loop {
                let Err(error) = failed else {
                    return failed;
                };
                self.load_missed_class(error, &mut attempts, true).await?;
                failed = handler(self, arguments.clone()).await;
                if !failed.as_ref().is_err_and(ClassMiss::is) {
                    if let Err(error) = &failed {
                        self.record_native_failure(error, NativeFrame::Function(name), &arguments);
                    }
                    return failed;
                }
            }
        })
    }

    /// The same for a native that calls back into PHP, whose arguments are
    /// in the trace of its frame (`site`): loads the class and calls it
    /// again with its frame published as the first call had it.
    #[cold]
    pub(crate) fn retry_traced_native(
        &mut self,
        name: &'static str,
        handler: AsyncFn,
        site: Option<crate::error_traces::NativeCallbackSite>,
        failed: NativeResult,
    ) -> LocalFuture<'_, NativeResult> {
        Box::pin(async move {
            let by_value = self.registry.get_function(name).is_some_and(|entry| {
                entry
                    .params
                    .iter()
                    .all(|parameter| !parameter.by_ref && !parameter.sensitive)
            });
            let arguments = site.and_then(|site| match site.native {
                crate::error_traces::NativeCaller::Saved(trace) if by_value => {
                    match trace.arguments {
                        Value::Array(values) => Some(
                            values
                                .iter()
                                .map(|(_, value)| value.clone())
                                .collect::<Vec<_>>(),
                        ),
                        _ => None,
                    }
                }
                _ => None,
            });
            let mut attempts = 0;
            let mut failed = failed;
            loop {
                let Err(error) = failed else {
                    return failed;
                };
                self.load_missed_class(error, &mut attempts, arguments.is_some())
                    .await?;
                let arguments = arguments.clone().unwrap_or_default();
                let native = self.native_function_trace(name, &arguments);
                self.native_callback_sites
                    .push(crate::error_traces::NativeCallbackSite {
                        boundary: self.call_stack.frames().len(),
                        native: crate::error_traces::NativeCaller::Saved(native),
                    });
                failed = handler(self, arguments).await;
                let site = self.native_callback_sites.pop();
                if !failed.as_ref().is_err_and(ClassMiss::is) {
                    if let Err(error) = &failed
                        && self.needs_native_failure_frame(error)
                    {
                        self.failed_native = site.and_then(|site| match site.native {
                            crate::error_traces::NativeCaller::Saved(trace) => Some(trace),
                            _ => None,
                        });
                    }
                    return failed;
                }
            }
        })
    }

    /// A native method's handler, after it failed with a class miss: loads
    /// the class and calls it again with the same receiver and arguments.
    #[cold]
    pub(crate) fn retry_native_method<'a>(
        &'a mut self,
        method: &'a crate::classes::Method,
        (handler, receiver): (NativeMethod, Option<Rc<Object>>),
        (arguments, named): (Vec<Value>, &'a [(usize, PhpString)]),
        failed: NativeResult,
    ) -> LocalFuture<'a, NativeResult> {
        Box::pin(async move {
            let mut attempts = 0;
            let mut failed = failed;
            loop {
                let Err(error) = failed else {
                    return failed;
                };
                self.load_missed_class(error, &mut attempts, true).await?;
                let again = arguments.clone();
                failed = match handler {
                    NativeMethod::Sync(handler) => handler(self, receiver.as_ref(), &again),
                    NativeMethod::Async(handler) => handler(self, receiver.clone(), again).await,
                    NativeMethod::Window(window) => {
                        (window.run)(self, receiver.clone(), again).await
                    }
                    NativeMethod::AsyncNamed(handler) => {
                        handler(self, receiver.clone(), again, named).await
                    }
                    NativeMethod::Abstract => {
                        Err(NativeError::error("Cannot call abstract method"))
                    }
                };
                if !failed.as_ref().is_err_and(ClassMiss::is) {
                    if let Err(error) = &failed {
                        self.record_native_failure(error, NativeFrame::Method(method), &arguments);
                    }
                    return failed;
                }
            }
        })
    }
}

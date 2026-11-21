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
//! and call the native again with the same arguments: the general boundary,
//! which can wait, does it itself ([`Vm::retry_missed_function`],
//! [`Vm::retry_missed_method`]), and the synchronous ones in the batch loop
//! put the call back as it was, so that the interpreter's own retry of the
//! `Call` finds it ([`Vm::restore_native_call`]).
//!
//! None of this costs a call that does not miss: the miss is seen in the
//! error branch the call has anyway, and everything after it is in the cold
//! functions below. The miss site keeps the arguments for the second call
//! in `Vm::missed_native` from where they are already: borrowed by a
//! synchronous handler, in the copy an async one keeps for the native frame
//! of a failure, or in the trace of a native that calls back into PHP. A
//! native called with none of these (as a callback of another native)
//! reports the missing class as an error instead.

use phera_extension_api::{FunctionEntry, LocalFuture, NativeError, NativeResult};
use phera_value::{PhpString, Value};

use crate::Vm;
use crate::autoload::ClassMiss;
use crate::calls::callback_binding::{ArgumentBinding, ArgumentOrigin};
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

    /// Keeps the arguments of a native call that missed a class, for its
    /// second call: the miss site does this, the general native boundary
    /// takes them (`Vm::finish_native_window_at`).
    #[cold]
    #[inline(never)]
    pub(crate) fn keep_missed_arguments(&mut self, arguments: &[Value]) {
        self.missed_native = Some(arguments.to_vec());
    }

    /// The same from the trace of a native that calls back into PHP, which
    /// keeps a copy of each argument: the same value for a parameter taken
    /// by value, so only for a native with no by-reference or sensitive
    /// parameter (the trace masks those).
    #[cold]
    #[inline(never)]
    pub(crate) fn keep_traced_arguments(
        &mut self,
        name: &str,
        trace: Option<&crate::error_traces::NativeTrace>,
    ) {
        let by_value = self.registry.get_function(name).is_some_and(|entry| {
            entry
                .params
                .iter()
                .all(|parameter| !parameter.by_ref && !parameter.sensitive)
        });
        if let (true, Some(Value::Array(values))) = (by_value, trace.map(|trace| &trace.arguments))
        {
            self.missed_native = Some(values.iter().map(|(_, value)| value.clone()).collect());
        }
    }

    /// A native function call that failed with a class miss: loads the class
    /// and calls the native again with the arguments the miss kept, or, when
    /// none were kept, reports the class as missing.
    #[cold]
    pub(crate) fn retry_missed_function(
        &mut self,
        entry: FunctionEntry,
        (origin, binding): (ArgumentOrigin, ArgumentBinding),
        error: NativeError,
    ) -> LocalFuture<'_, NativeResult> {
        Box::pin(async move {
            let mut attempts = 0;
            let mut error = error;
            loop {
                let arguments = self.missed_native.take();
                self.load_missed_class(error, &mut attempts, arguments.is_some())
                    .await?;
                let arguments = arguments.unwrap_or_default();
                match self
                    .native_entry_origin(entry, arguments, origin, binding)
                    .await
                {
                    Err(failure) if ClassMiss::is(&failure) => error = failure,
                    result => return result,
                }
            }
        })
    }

    /// The same for a native method.
    #[cold]
    pub(crate) fn retry_missed_method<'a>(
        &'a mut self,
        (method, context): (
            &'a crate::classes::Method,
            &'a crate::classes::MethodContext,
        ),
        (named, origin): (&'a [(usize, PhpString)], ArgumentOrigin),
        error: NativeError,
    ) -> LocalFuture<'a, NativeResult> {
        Box::pin(async move {
            let mut attempts = 0;
            let mut error = error;
            loop {
                let arguments = self.missed_native.take();
                self.load_missed_class(error, &mut attempts, arguments.is_some())
                    .await?;
                let arguments = arguments.unwrap_or_default();
                match self
                    .native_method_window(method, context, arguments, named, origin)
                    .await
                {
                    Err(failure) if ClassMiss::is(&failure) => error = failure,
                    result => return result,
                }
            }
        })
    }
}

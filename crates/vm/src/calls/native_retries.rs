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
//! None of this costs a call that does not miss: the arguments for the
//! second call come back with the miss, from where they are anyway (the
//! synchronous handler only borrowed them; an async one's are the copy kept
//! for the native frame of a failure, or the trace of a native that calls
//! back into PHP). A native called with neither (as a callback of another
//! native) reports the missing class as an error instead.

use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;
use crate::autoload::ClassMiss;
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
    /// window keeps below its first register (`$this`, a closure), moved
    /// out with [`Vm::take_call_prefix`], and the arguments. For a class
    /// miss from the native, after which the interpreter runs the `Call`
    /// again.
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

    /// Moves out what the window of `call` keeps below its first register,
    /// so that [`Vm::restore_native_call`] can put it back.
    #[inline]
    pub(crate) fn take_call_prefix(&mut self, call: &PreparedCall) -> [Option<Value>; 2] {
        let (start, base) = (call.start(), call.base());
        [start, start + 1].map(|slot| {
            if slot < base {
                self.registers
                    .window_mut(slot, 1)
                    .first_mut()
                    .and_then(Option::take)
            } else {
                None
            }
        })
    }
}

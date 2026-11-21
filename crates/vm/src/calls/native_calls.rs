//! Calls of native functions with a record of a few numbers.
//!
//! ```php
//! namespace App;
//! if (str_starts_with($path, '/api')) { ... }   // InitCall, two sends, Call
//! ```
//!
//! A call to a synchronous native function is prepared as
//! `PreparedCallee::Native(id)`: the function's number in the registry's
//! flat list, and nothing else. Its sends check the registry entry's
//! parameters, and its `Call` runs the function from the batch loop when
//! every argument already has a type the function takes. Anything else
//! (a named argument, a by-reference parameter, an argument to convert, a
//! spread) is the general path's: before that path touches the call, the
//! record is given the full description the general path reads
//! ([`Vm::describe_native_call`]), and the call goes on as before.

use phera_bytecode::{Instruction, Reg};
use phera_extension_api::{FunctionEntry, NativeError, NativeFn, NativeId};
use phera_value::Value;

use crate::Vm;
use crate::calls::call_state::{CallTarget, Callee, ParameterSpec};
use crate::calls::callback_binding::ArgumentBinding;
use crate::calls::execution_loop::BatchExit;
use crate::calls::prepared_call::{CallDetails, PreparedCallee};

/// Arguments a lean native call passes from an array on the Rust stack.
const SHORT_ARGUMENTS: usize = 4;

impl Vm {
    /// The description of a native function that the general path reads, as
    /// linking a name to it makes it.
    pub(crate) fn native_callee(entry: FunctionEntry) -> Callee {
        Callee {
            argument_binding: ArgumentBinding::Caller,
            name: entry.name.into(),
            parameters: entry
                .params
                .iter()
                .map(ParameterSpec::native)
                .collect::<Vec<_>>()
                .into(),
            target: CallTarget::Native(entry),
        }
    }

    /// The native function the innermost prepared call names, if it is a
    /// lean native call.
    fn top_native(&self) -> Option<NativeId> {
        match self.call_stack.last_call()?.callee {
            PreparedCallee::Native(id) => Some(id),
            _ => None,
        }
    }

    /// Gives a lean native call its full description, before the general
    /// path handles an instruction that works on it.
    #[cold]
    #[inline(never)]
    pub(crate) fn describe_native_call(&mut self, instruction: &Instruction) {
        if !matches!(
            instruction,
            Instruction::SendValue { .. }
                | Instruction::SendLocal { .. }
                | Instruction::SendPath { .. }
                | Instruction::SendSpread { .. }
                | Instruction::NameArgument { .. }
                | Instruction::Call { .. }
                | Instruction::MakeCallable { .. }
                | Instruction::SendUserCallback { .. }
        ) {
            return;
        }
        let Some(id) = self.top_native() else {
            return;
        };
        let Some(entry) = self.registry.function_at(id).copied() else {
            return;
        };
        let details = CallDetails {
            callee: Some(Self::native_callee(entry)),
            ..CallDetails::default()
        };
        self.call_stack.describe_top_call(details);
    }

    /// `Call` of a lean native call, from the batch loop: the function runs
    /// at once when its arguments fit its parameters as they are; otherwise
    /// the call gets its full description and the general native call
    /// binds the arguments.
    pub(crate) fn call_lean_native(
        &mut self,
        destination: Reg,
    ) -> Result<Option<BatchExit>, NativeError> {
        let call = *self.call_stack.top_call()?;
        let PreparedCallee::Native(id) = call.callee else {
            return self.call_native(destination);
        };
        let entry = self
            .registry
            .function_at(id)
            .copied()
            .ok_or_else(|| internal_error!("native function number"))?;
        let count = call.sent as usize;
        let handler = match entry.handler {
            NativeFn::Sync(handler)
                if call.is_plain() && self.accepts_sync_call(entry, call.base(), count) =>
            {
                handler
            }
            _ => {
                self.describe_native_call(&Instruction::Call {
                    dst: destination,
                    result: phera_bytecode::CallResult::Used,
                });
                return self.call_native(destination);
            }
        };
        // The values move out of the registers, so that the handler can
        // borrow the whole VM: up to four into an array here, more into the
        // reusable native buffer. `accepts_sync_call` has checked that every
        // one of them is defined.
        if count <= SHORT_ARGUMENTS {
            let mut arguments = [const { Value::Null }; SHORT_ARGUMENTS];
            for (argument, slot) in arguments
                .iter_mut()
                .zip(self.registers.window_mut(call.base(), count))
            {
                if let Some(value) = slot.take() {
                    *argument = value;
                }
            }
            self.call_stack.pop_call()?;
            self.registers.truncate(call.start());
            let arguments = arguments.get(..count).unwrap_or_default();
            return self.invoke_sync_function(
                entry.name,
                handler,
                arguments,
                destination,
                ArgumentBinding::Caller,
            );
        }
        let mut arguments = self.call_stack.native_arguments();
        arguments.reserve(count);
        for slot in self.registers.window_mut(call.base(), count) {
            if let Some(value) = slot.take() {
                arguments.push(value);
            }
        }
        self.call_stack.pop_call()?;
        self.registers.truncate(call.start());
        let result = self.invoke_sync_function(
            entry.name,
            handler,
            &arguments,
            destination,
            ArgumentBinding::Caller,
        );
        self.call_stack.recycle_native_arguments(arguments);
        result
    }
}

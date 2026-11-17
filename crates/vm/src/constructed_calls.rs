//! Constructor-free calls complete synchronously without entering the async call dispatcher.

use crate::{Vm, call_state::CallTarget};
use phera_bytecode::Reg;
use phera_value::Value;

impl Vm {
    /// Mirrors finish_call's Constructed arm after every argument has already evaluated.
    /// This guard retains the ordinary path for named routing and real constructors.
    pub(crate) fn complete_constructed_call(&mut self, destination: Reg) -> bool {
        let Some(frame) = self.call_stack.current_mut() else {
            return false;
        };
        if !frame.calls.last().is_some_and(|call| {
            matches!(call.callee.target, CallTarget::Constructed(_)) && call.extra_named.is_none()
        }) {
            return false;
        }
        let Some(mut call) = frame.calls.pop() else {
            return false;
        };
        let CallTarget::Constructed(object) = &call.callee.target else {
            frame.calls.push(call);
            return false;
        };
        frame.set(
            &mut self.registers,
            destination,
            Value::Object(object.clone()),
        );
        call.arguments.clear();
        self.call_stack.recycle_arguments(call.arguments);
        self.call_stack.recycle_bound_callee(call.callee);
        true
    }
}

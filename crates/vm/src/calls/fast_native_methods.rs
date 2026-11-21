//! Native methods prepared by the interpreter loop; a synchronous one can
//! finish directly from its argument window.
//!
//! ```php
//! $stack->push($value); // one by-value argument
//! $value = $stack->pop(); // empty stacks use the ordinary exception path
//! ```

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_extension_api::MethodEntry;

use super::{
    call_state::{CallTarget, Callee},
    callback_binding::ArgumentBinding,
    execution_loop::{Machine, Position, Step},
    prepared_call::{CallFlags, PreparedCall},
};
use crate::classes::{Method, MethodBody, MethodContext};

impl Machine<'_> {
    /// Retain the ordinary call details for named, missing or unhandled arguments.
    pub(super) fn prepare_direct_method(
        &mut self,
        position: Position,
        context: MethodContext,
        method: Rc<Method>,
        entry: MethodEntry,
    ) -> Step {
        // A method whose direct handler declines a call (an argument it
        // does not take, a case it leaves to the method) runs as prepared
        // here, sync or async, from the batch loop.
        if entry.direct.is_none()
            || entry.preparation.is_some()
            || entry
                .params
                .iter()
                .any(|param| param.by_ref || param.variadic)
        {
            return self.prepare_native_method(position, context, method);
        }
        let signature = method.signature(self.code);
        let callee = Callee {
            name: signature.name.clone(),
            parameters: signature.parameters.clone(),
            argument_binding: ArgumentBinding::Caller,
            target: CallTarget::Method { method, context },
        };
        super::pending::prepare_general(self.stack, self.registers, callee);
        if let Some(call) = self.stack.last_call_mut() {
            call.flags.insert(CallFlags::DIRECT_NATIVE);
        }
        Step::Done(position.ip)
    }

    /// `InitMethodCall` of any other public native method: the call's
    /// description, as the VM's method preparation makes it. The call
    /// itself runs the method from the batch loop.
    pub(super) fn prepare_native_method(
        &mut self,
        position: Position,
        context: MethodContext,
        method: Rc<Method>,
    ) -> Step {
        let signature = method.signature(self.code);
        let callee = Callee {
            name: signature.name.clone(),
            parameters: signature.parameters.clone(),
            argument_binding: ArgumentBinding::Caller,
            target: CallTarget::Method { method, context },
        };
        super::pending::prepare_general(self.stack, self.registers, callee);
        Step::Done(position.ip)
    }

    pub(super) fn finish_direct_method(
        &mut self,
        position: Position,
        destination: Reg,
        call: PreparedCall,
    ) -> Step {
        if !call.flags.contains(CallFlags::DIRECT_NATIVE) || !call.is_plain() {
            return Step::Slow;
        }
        let Some(callee) = self
            .stack
            .top_details()
            .filter(|details| details.named.is_none())
            .and_then(|details| details.callee.as_ref())
        else {
            return Step::Slow;
        };
        let CallTarget::Method { method, context } = &callee.target else {
            return Step::Slow;
        };
        let MethodBody::Native(entry) = method.body else {
            return Step::Slow;
        };
        let (Some(direct), Some(receiver)) = (entry.direct, context.receiver.as_deref()) else {
            return Step::Slow;
        };
        let required = entry.params.iter().filter(|param| !param.optional).count();
        let sent = call.sent as usize;
        if sent < required || sent > entry.params.len() {
            return Step::Slow;
        }
        let operands = self.registers.window(call.base(), call.sent as usize);
        if operands.len() != call.sent as usize
            || !super::native_signature::accepts(entry.params, operands)
        {
            return Step::Slow;
        }
        let Some(value) = direct(receiver, operands) else {
            return Step::Slow;
        };
        self.stack.pop_plain();
        self.registers.truncate(call.start());
        self.registers
            .put(position.frame.base() + destination.index(), value);
        if self.spend_work() {
            self.publish(position.ip);
            return Step::Yield;
        }
        Step::Done(position.ip)
    }
}

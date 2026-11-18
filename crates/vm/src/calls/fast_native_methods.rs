//! A synchronous native method can finish directly from its argument window.
//!
//! ```php
//! $stack->push($value); // one by-value argument
//! $value = $stack->pop(); // empty stacks use the ordinary exception path
//! ```

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_extension_api::{MethodEntry, NativeMethod, ParamType};

use super::{
    call_state::{CallTarget, Callee},
    callback_binding::ArgumentBinding,
    execution_loop::{Machine, Position, Step},
    prepared_call::{CallFlags, PreparedCall},
};
use crate::classes::{Method, MethodBody, MethodContext};

impl Machine<'_> {
    /// Retain the ordinary call details for named, missing or unhandled arguments.
    #[inline(never)]
    pub(super) fn prepare_direct_method(
        &mut self,
        position: Position,
        context: MethodContext,
        method: Rc<Method>,
        entry: MethodEntry,
    ) -> Step {
        if entry.direct.is_none()
            || entry.preparation.is_some()
            || !matches!(entry.handler, NativeMethod::Sync(_))
            || entry.params.iter().any(|param| {
                param.ty != ParamType::Mixed || param.by_ref || param.variadic || param.optional
            })
        {
            return Step::Slow;
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

    #[inline(never)]
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
        if entry.params.len() != call.sent as usize {
            return Step::Slow;
        }
        let operands = self.registers.window(call.base(), call.sent as usize);
        if operands.iter().any(Option::is_none) {
            return Step::Slow;
        }
        let Some(value) = direct(receiver, operands) else {
            return Step::Slow;
        };
        self.stack.pop_plain();
        self.registers.truncate(call.start());
        self.registers
            .put(position.frame.base() + destination.index(), value);
        self.spend_work();
        Step::Done(position.ip)
    }
}

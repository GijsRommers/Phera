//! A native method may complete directly from a prepared argument window.
//!
//! ```php
//! $generator->send($value);
//! ```
//!
//! Positional, untyped arguments already hold their PHP values. Missing
//! arguments, named arguments, and methods that need to await keep the full
//! native-method path, including its argument errors.

use phera_bytecode::{NameId, Reg, Visibility};
use phera_extension_api::{NativeError, NativeMethod, ParamType};
use phera_value::Value;

use crate::{
    Vm,
    calls::{
        call_state::{CallTarget, Callee},
        pending::prepare_general,
        prepared_call::CallFlags,
    },
    classes::{Invocation, MethodBody, MethodContext},
};

impl Vm {
    /// The borrowed method loop already resolved the receiver's linked table
    /// before handing a native call back to the VM.
    #[inline(never)]
    pub(crate) fn prepare_ready_native_method(&mut self, receiver: Reg, name: NameId) -> bool {
        let Some(frame) = self.current() else {
            return false;
        };
        let slot = frame.base() + receiver.index();
        let [Some(Value::Object(object))] = self.registers.window(slot, 1) else {
            return false;
        };
        let Some(class) = self.classes.for_object(object) else {
            return false;
        };
        let key = self.code.unit_name_key(frame.frame.function, name);
        let Some(found) = self
            .classes
            .cached_method_table(&class)
            .and_then(|table| table.get(key))
        else {
            return false;
        };
        let MethodBody::Native(entry) = found.method.body else {
            return false;
        };
        if !matches!(entry.handler, NativeMethod::Window(_))
            || found.method.visibility != Visibility::Public
            || found.method.is_static
            || frame.scope().is_some_and(|scope| class.is_a(&scope.name))
        {
            return false;
        }
        let Some(scope) = found.scope.upgrade() else {
            return false;
        };
        let signature = found.method.signature(&self.code);
        let target = CallTarget::Method {
            method: found.method.clone(),
            context: MethodContext {
                receiver: Some(object.clone()),
                scope,
                called: class.clone(),
                invocation: Invocation::Method,
            },
        };
        let callee = Callee {
            argument_binding: crate::calls::callback_binding::ArgumentBinding::Caller,
            name: signature.name.clone(),
            parameters: signature.parameters.clone(),
            target,
        };
        if receiver.index() >= frame.function().locals.len() {
            self.registers.take(slot);
        }
        prepare_general(&mut self.call_stack, &mut self.registers, callee);
        if let [parameter] = entry.params
            && matches!(parameter.ty, ParamType::Mixed)
            && !parameter.by_ref
            && !parameter.variadic
            && let Some(call) = self.call_stack.last_call_mut()
        {
            call.flags.insert(CallFlags::WINDOW_ARGUMENT);
        }
        true
    }

    pub(crate) fn send_window_argument(&mut self, source: Reg) -> Result<bool, NativeError> {
        let call = *self.call_stack.top_call()?;
        if !call.flags.contains(CallFlags::WINDOW_ARGUMENT)
            || call.sent != 0
            || self
                .call_stack
                .top_details()
                .is_some_and(|details| details.named.is_some())
        {
            return Ok(false);
        }
        let Some(value) = self.read_defined_local(source) else {
            return Ok(false);
        };
        self.registers.put(call.base(), value);
        self.call_stack.top_call_mut()?.mark_sent(0, true);
        Ok(true)
    }

    pub(crate) fn finish_ready_native_window(&mut self) -> Result<Option<Value>, NativeError> {
        let call = *self.call_stack.top_call()?;
        if !call.is_plain() {
            return Ok(None);
        }
        let Some(target) = self
            .call_stack
            .top_details()
            .and_then(|details| details.callee.as_ref())
            .map(|callee| &callee.target)
        else {
            return Ok(None);
        };
        let CallTarget::Method { method, context } = target else {
            return Ok(None);
        };
        let MethodBody::Native(entry) = &method.body else {
            return Ok(None);
        };
        let NativeMethod::Window(implementation) = entry.handler else {
            return Ok(None);
        };
        if entry.preparation.is_some() || context.invocation != crate::classes::Invocation::Method {
            return Ok(None);
        }
        let argument = match (entry.params, call.sent) {
            ([], 0) => None,
            ([parameter], 1)
                if matches!(parameter.ty, ParamType::Mixed)
                    && !parameter.by_ref
                    && !parameter.variadic =>
            {
                let [Some(value)] = self.registers.window(call.base(), 1) else {
                    return Ok(None);
                };
                Some(value.clone())
            }
            _ => return Ok(None),
        };
        let Some(receiver) = context.receiver.clone() else {
            return Ok(None);
        };
        let Some(result) = (implementation.try_ready)(self, &receiver, argument.as_ref()) else {
            return Ok(None);
        };
        self.call_stack.pop_call()?;
        self.registers.truncate(call.start());
        result.map(Some)
    }
}

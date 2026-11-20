//! Preparing calls to PHP code: functions, methods and closures.
//!
//! ```php
//! $total = sum(1, 2);        // [ 1, 2, locals ]
//! $point->moveBy(3);         // [ $point | 3, locals ]
//! $scale(4);                 // [ $scale, $this | 4, captured $factor, locals ]
//! ```
//!
//! Each is one [`PreparedCall`] and a window at the top of the register
//! stack. A method's window starts with `$this`, a closure's with the
//! closure object, its bound `$this`, and after the parameters its captured
//! values. Nothing else is built: the class scope and the called class
//! follow from these (see `window_context`).
//!
//! A general [`Callee`] for PHP code (from a callable value, or a method the
//! general path resolved) becomes the same record, and turns back into a
//! [`Callee`] when `f(...)` makes a closure of it instead of calling it.

use std::rc::Rc;

use phera_extension_api::NativeError;
use phera_value::{Object, Value};

use crate::Vm;
use crate::calls::call_stack::CallStack;
use crate::calls::call_state::{CallTarget, Callee};
use crate::calls::callback_binding::ArgumentBinding;
use crate::calls::pending::php_window;
use crate::calls::prepared_call::{CallFlags, CallKind, PreparedCall, PreparedCallee};
use crate::calls::registers::Registers;
use crate::classes::{Invocation, MethodBody};
use crate::closures::ClosureState;
use crate::code_table::{CodeTable, FunctionIndex};

/// Reserve the window of a call of `kind` to `function` and record the
/// call. `below` are what the kind keeps below the first parameter, in
/// order: a method's `$this`; a closure's object and its `$this`.
#[inline]
pub(crate) fn push_php_call(
    code: &CodeTable,
    (stack, registers): (&mut CallStack, &mut Registers),
    function: FunctionIndex,
    kind: CallKind,
    below: impl IntoIterator<Item = Option<Value>>,
) {
    let start = registers.len();
    registers.grow_to(start + kind.prefix() + php_window(code, function));
    for (offset, value) in below.into_iter().enumerate() {
        if let Some(value) = value {
            registers.put(start + offset, value);
        }
    }
    let call = PreparedCall::of_kind(PreparedCallee::Php(function), kind, start);
    stack.push_call(call, None);
}

/// Prepare the call that `new` of a class without a constructor makes: its
/// arguments are evaluated and dropped, and it gives the object.
#[inline]
pub(crate) fn push_constructed(
    (stack, registers): (&mut CallStack, &mut Registers),
    object: Rc<Object>,
) {
    let start = registers.len();
    registers.grow_to(start + 2);
    registers.put(start, Value::Object(object));
    let call = PreparedCall::of_kind(PreparedCallee::Constructed, CallKind::Constructor, start);
    stack.push_call(call, None);
}

/// Prepare a call to a closure over a PHP function: the object and its
/// bound `$this` below the parameters, the captured values after them.
/// Returns `false`, having done nothing, for a closure over native code.
pub(crate) fn push_closure_call(
    code: &CodeTable,
    (stack, registers): (&mut CallStack, &mut Registers),
    object: &Rc<Object>,
) -> bool {
    let native = object.native.borrow();
    let Some(state) = native
        .as_ref()
        .and_then(|state| state.downcast_ref::<ClosureState>())
    else {
        return false;
    };
    let Some(function) = state.callee.user_function() else {
        return false;
    };
    let this = state
        .context
        .as_ref()
        .and_then(|context| context.receiver.clone());
    let below = [Some(Value::Object(object.clone())), this.map(Value::Object)];
    push_php_call(code, (stack, registers), function, CallKind::Closure, below);
    let Some(call) = stack.last_call() else {
        return false;
    };
    let captures = call.base() + code.function(function).parameters.len();
    for (index, value) in state.captures.iter().enumerate() {
        if let Some(value) = value {
            registers.put(captures + index, value.clone());
        }
    }
    true
}

impl Vm {
    /// Prepare a call to a general callee that is PHP code as a plain
    /// record. Returns the callee back when it is not PHP code.
    pub(crate) fn prepare_php_callee(&mut self, callee: Callee) -> Result<(), Callee> {
        let parts = (&mut self.call_stack, &mut self.registers);
        let prepared = match &callee.target {
            CallTarget::User(function) => {
                push_php_call(&self.code, parts, *function, CallKind::Function, []);
                true
            }
            CallTarget::Method { method, context } => match method.body {
                MethodBody::User(function) => {
                    let kind = match context.invocation {
                        Invocation::Constructor => CallKind::Constructor,
                        Invocation::Method => CallKind::Method,
                    };
                    let this = context.receiver.clone().map(Value::Object);
                    push_php_call(&self.code, parts, function, kind, [this]);
                    let scope = self.code.facts(function).class;
                    if let Some(call) = self.call_stack.last_call_mut()
                        && context.receiver.is_none()
                        && scope != Some(context.called.index)
                    {
                        call.called = Some(context.called.index);
                    }
                    true
                }
                _ => false,
            },
            CallTarget::Closure(object) => push_closure_call(&self.code, parts, object),
            _ => false,
        };
        if !prepared {
            return Err(callee);
        }
        if matches!(callee.argument_binding, ArgumentBinding::Internal)
            && let Some(call) = self.call_stack.last_call_mut()
        {
            call.flags.insert(CallFlags::INVOKE);
        }
        Ok(())
    }

    /// The general description of a prepared call to PHP code, read back
    /// from its record and window.
    pub(crate) fn php_callee(
        &self,
        function: FunctionIndex,
        call: &PreparedCall,
    ) -> Result<Callee, NativeError> {
        let context = self.prepared_context(function, call);
        let mut callee = self.function_callee(function);
        if call.flags.contains(CallFlags::INVOKE) {
            callee.argument_binding = ArgumentBinding::Internal;
            callee.name = "Closure::__invoke".into();
        }
        match call.kind {
            CallKind::Function => {}
            CallKind::Closure => {
                let object = context
                    .closure()
                    .ok_or_else(|| internal_error!("closure"))?;
                callee.target = CallTarget::Closure(object.clone());
            }
            CallKind::Method | CallKind::Constructor => {
                let method_context = context.method().ok_or_else(|| internal_error!("method"))?;
                let method = method_context
                    .scope
                    .methods
                    .values()
                    .find(
                        |method| matches!(method.body, MethodBody::User(body) if body == function),
                    )
                    .cloned()
                    .ok_or_else(|| internal_error!("method body"))?;
                let signature = method.signature(&self.code);
                callee.name = signature.name.clone();
                callee.parameters = signature.parameters.clone();
                callee.target = CallTarget::Method {
                    method,
                    context: method_context,
                };
            }
        }
        Ok(callee)
    }
}

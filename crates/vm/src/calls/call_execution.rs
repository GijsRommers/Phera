//! `Call` turns a prepared argument window into a running frame.
//!
//! The arguments are already in the callee's registers (`Send` put them
//! there). Entering checks what can only be checked once every argument is
//! known (missing arguments, deferred type coercions, variadics), records the
//! class context and closure in the frame's side table, and pushes a frame.
//! User code never recurses through the Rust stack.

use std::rc::Rc;

use phera_bytecode::{FunctionId, Reg};
use phera_extension_api::NativeError;
use phera_value::Object;

use crate::Vm;
use crate::calls::call_state::{CallTarget, PendingCall};
use crate::calls::execution_loop::BatchExit;
use crate::calls::frame::Frame;
use crate::classes::{MethodBody, MethodContext};
use crate::code_table::FunctionIndex;

/// The function a call enters and the state its frame keeps beside it.
pub(crate) struct Entry {
    pub(crate) function: FunctionIndex,
    pub(crate) method: Option<MethodContext>,
    pub(crate) closure: Option<Rc<Object>>,
}

impl Vm {
    /// Enters the prepared call if its target is PHP code. Returns `false`
    /// for a native target, which the async shell runs instead.
    pub(crate) fn enter_prepared_call(
        &mut self,
        destination: Option<Reg>,
    ) -> Result<bool, NativeError> {
        let call = self.call_stack.pending()?;
        if call.callee.user_function().is_none()
            && !matches!(call.callee.target, CallTarget::Closure(_))
        {
            return Ok(false);
        }
        self.check_frame_limit()?;
        let call = self.call_stack.take_pending()?;
        self.enter_window(call, destination)?;
        Ok(true)
    }

    /// `CallDirect` when the interpreter loop's quick entry does not apply
    /// (an argument to convert, too few arguments, a variadic): send the
    /// argument registers as an ordinary prepared call and enter that.
    pub(crate) fn call_direct_general(
        &mut self,
        declared: FunctionId,
        args: Reg,
        count: u16,
        destination: Reg,
        work: &mut usize,
    ) -> Result<Option<BatchExit>, NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("call without frame"))?;
        let from = frame.base() + args.index();
        let first = self.code.facts(frame.frame.function).unit_first;
        let (function, _) = self
            .code
            .declared_facts(first, declared.index())
            .ok_or_else(|| internal_error!("direct call target"))?;
        self.prepare_call(self.function_callee(function))?;
        for index in 0..count as usize {
            let value = self.registers.take(from + index);
            self.store_sent_value(index, value)?;
        }
        *work += 1;
        self.check_frame_limit()?;
        let call = self.call_stack.take_pending()?;
        self.enter_window(call, Some(destination))?;
        Ok(None)
    }

    /// Bind the arguments, then push the frame. The frame is pushed even when
    /// binding fails, so that the error's trace shows the call.
    fn enter_window(
        &mut self,
        call: PendingCall,
        destination: Option<Reg>,
    ) -> Result<(), NativeError> {
        let PendingCall {
            callee,
            mut window,
            owner,
        } = call;
        let origin = callee.argument_binding.origin(self.argument_origin);
        let entry = self.entry_for_target(callee.target, owner, window.base)?;
        let arguments = self.save_window_arguments(entry.function, &window);
        let result = self.with_argument_origin(origin, |vm| vm.bind_window(&entry, &window));
        if window.extra_named.capacity() > 0 {
            window.extra_named.clear();
            self.call_stack.argument_names.push(window.extra_named);
        }
        let frame = Frame::new(entry.function, window.base, destination);
        self.push_frame(entry, frame);
        if let Some(arguments) = arguments {
            self.call_stack.current_extras_mut().arguments = arguments;
        }
        result
    }

    /// Push a frame and store its class context and closure beside it.
    pub(crate) fn push_frame(&mut self, entry: Entry, frame: Frame) {
        self.call_stack.push(frame);
        if entry.method.is_some() || entry.closure.is_some() {
            let extras = self.call_stack.current_extras_mut();
            extras.method = entry.method;
            extras.closure = entry.closure;
        }
    }

    /// What a call target enters: a function, a method with its context,
    /// or a closure with its captures copied into the window.
    fn entry_for_target(
        &mut self,
        target: CallTarget,
        owner: Option<Rc<Object>>,
        base: usize,
    ) -> Result<Entry, NativeError> {
        match target {
            CallTarget::Closure(object) => self.closure_entry(object, base),
            CallTarget::User(function) => Ok(Entry {
                function,
                method: None,
                closure: owner,
            }),
            CallTarget::Method { method, context } => {
                let MethodBody::User(function) = method.body else {
                    return Err(internal_error!("native method in PHP entry"));
                };
                Ok(Entry {
                    function,
                    method: Some(context),
                    closure: owner,
                })
            }
            _ => Err(internal_error!("native target in PHP entry")),
        }
    }

    /// Captures belong to the closure object and are copied into the
    /// registers after the parameters.
    fn closure_entry(&mut self, object: Rc<Object>, base: usize) -> Result<Entry, NativeError> {
        let native = object.native.borrow();
        let state = native
            .as_ref()
            .and_then(|value| value.downcast_ref::<crate::closures::ClosureState>())
            .ok_or_else(|| internal_error!("closure state"))?;
        let function = state
            .callee
            .user_function()
            .ok_or_else(|| internal_error!("closure function"))?;
        let method = state.context.clone();
        let offset = base + self.code.function(function).parameters.len();
        for (index, value) in state.captures.iter().enumerate() {
            if let Some(value) = value {
                self.registers.grow_to(offset + index + 1);
                self.registers.window_mut(offset + index, 1)[0] = Some(value.clone());
            }
        }
        drop(native);
        Ok(Entry {
            function,
            method,
            closure: Some(object),
        })
    }

    /// PHP frame depth is independent of the Rust stack and checked before committing entry.
    pub(crate) fn check_frame_limit(&self) -> Result<(), NativeError> {
        if self.call_stack.depth() >= self.frame_limit {
            return Err(NativeError::error(format!(
                "Maximum call stack depth of {} exceeded",
                self.frame_limit,
            )));
        }
        Ok(())
    }
}

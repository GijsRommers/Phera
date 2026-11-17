//! Call commits a filled window; user bytecode never recurses through Rust.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;

use crate::Vm;
use crate::calls::call_state::{CallTarget, PendingCall};

impl Vm {
    /// The interpreter handles PHP entry synchronously, keeping native futures off the hot path.
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

    /// Install scope and captures before parameter checks; failed entry still owns a trace frame.
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
        let mut frame = self.frame_for_target(callee.target, owner, destination, window.base)?;
        self.save_window_arguments(&mut frame, &window);
        let result = if window.checks == crate::calls::call_state::TypeChecks::Deferred {
            self.with_argument_origin(origin, |vm| vm.bind_window(&mut frame, &window))
        } else {
            self.bind_window(&mut frame, &window)
        };
        if window.extra_named.capacity() > 0 {
            window.extra_named.clear();
            self.call_stack.argument_names.push(window.extra_named);
        }
        self.call_stack.push(frame);
        result
    }

    /// Transfer bound method ownership into the activation instead of retaining it twice.
    fn frame_for_target(
        &mut self,
        target: CallTarget,
        owner: Option<std::rc::Rc<phera_value::Object>>,
        destination: Option<Reg>,
        base: usize,
    ) -> Result<crate::calls::frame::Frame, NativeError> {
        match target {
            CallTarget::Closure(object) => self.frame_for_closure(object, destination, base),
            CallTarget::User { function } => {
                Ok(self.frame_for_function(function, None, owner, destination, base))
            }
            CallTarget::Method { method, context } => {
                let Some(function) = method.function else {
                    return Err(internal_error!("native method in PHP entry"));
                };
                Ok(self.frame_for_function(function, Some(context), owner, destination, base))
            }
            _ => Err(internal_error!("native target in PHP entry")),
        }
    }

    /// Function metadata is borrowed while ownership of the receiver moves to the frame.
    fn frame_for_function(
        &mut self,
        function: crate::code_table::FunctionIndex,
        context: Option<crate::classes::MethodContext>,
        owner: Option<std::rc::Rc<phera_value::Object>>,
        destination: Option<Reg>,
        base: usize,
    ) -> crate::calls::frame::Frame {
        let frame = self.make_indexed_frame(function, destination, base);
        self.install_frame_method(&frame, context);
        self.call_stack.extras[frame.depth].closure = owner;
        frame
    }

    /// Captures belong to the callable object and are copied into its invocation's locals.
    fn frame_for_closure(
        &mut self,
        object: std::rc::Rc<phera_value::Object>,
        destination: Option<Reg>,
        base: usize,
    ) -> Result<crate::calls::frame::Frame, NativeError> {
        let native = object.native.borrow();
        let state = native
            .as_ref()
            .and_then(|value| value.downcast_ref::<crate::closures::ClosureState>())
            .ok_or_else(|| internal_error!("closure state"))?;
        let function = state
            .callee
            .user_function()
            .ok_or_else(|| internal_error!("closure function"))?;
        let frame = self.make_indexed_frame(function, destination, base);
        self.install_frame_method(&frame, state.context.clone());
        let offset = frame.function(&self.code).parameters.len();
        for (index, value) in state.captures.iter().enumerate() {
            if let Some(value) = value {
                frame.set(
                    &mut self.registers,
                    Reg((offset + index) as u16),
                    value.clone(),
                );
            }
        }
        drop(native);
        self.call_stack.extras[frame.depth].closure = Some(object);
        Ok(frame)
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

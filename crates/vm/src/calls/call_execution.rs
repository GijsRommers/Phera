//! `Call` turns a prepared argument window into a running frame.
//!
//! The arguments are already in the callee's registers (`Send` put them
//! there). Entering checks what can only be checked once every argument is
//! known (missing arguments, deferred type coercions, variadics) and pushes
//! a frame of the call's kind; `$this` and a closure are in the window
//! already. User code never recurses through the Rust stack.

use phera_bytecode::{FunctionId, Reg};
use phera_extension_api::NativeError;

use crate::Vm;
use crate::calls::call_view::CallView;
use crate::calls::callback_binding::ArgumentOrigin;
use crate::calls::execution_loop::BatchExit;
use crate::calls::frame::Frame;
use crate::calls::prepared_call::{CallDetails, CallFlags, PreparedCall, PreparedCallee};
use crate::calls::window_context::WindowContext;
use crate::code_table::FunctionIndex;

pub(crate) enum CallEntry {
    Php,
    Native,
    ConvertStrings,
}

impl Vm {
    /// Enter ordinary PHP frames directly. Native calls and string conversions
    /// that must run PHP code are left for the asynchronous interpreter.
    pub(crate) fn enter_prepared_call(
        &mut self,
        destination: Option<Reg>,
    ) -> Result<CallEntry, NativeError> {
        let call = self.call_stack.top_call()?;
        if call.callee == PreparedCallee::General {
            if !call.flags.contains(CallFlags::MAGIC) {
                return Ok(CallEntry::Native);
            }
            self.expand_magic_call()?;
        } else if call.flags.contains(CallFlags::STRING_ARGUMENT)
            && self.deferred_string_coercion(self.argument_origin)
        {
            return Ok(CallEntry::ConvertStrings);
        }
        self.check_frame_limit()?;
        let (call, details) = self.call_stack.pop_call()?;
        self.enter_call(call, details, destination)?;
        Ok(CallEntry::Php)
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
        self.prepare_php_call(function);
        for index in 0..count as usize {
            let value = self.registers.take(from + index);
            self.store_sent_value(index, value)?;
        }
        *work += 1;
        match self.enter_prepared_call(Some(destination))? {
            CallEntry::Php => Ok(None),
            CallEntry::ConvertStrings => Ok(Some(BatchExit::BindCall(destination))),
            CallEntry::Native => Err(internal_error!("native direct PHP call")),
        }
    }

    /// Bind the arguments, then push the frame. The frame is pushed even when
    /// binding fails, so that the error's trace shows the call.
    fn enter_call(
        &mut self,
        call: PreparedCall,
        details: Option<CallDetails>,
        destination: Option<Reg>,
    ) -> Result<(), NativeError> {
        let PreparedCallee::Php(function) = call.callee else {
            return Err(internal_error!("native target in PHP entry"));
        };
        let slots = CallView::new(&self.code, call, details.as_ref()).slots();
        let extra_named = details
            .map(|details| details.extra_named)
            .unwrap_or_default();
        let origin = match call.flags.contains(CallFlags::INVOKE) {
            true => ArgumentOrigin::Callback,
            false => self.argument_origin,
        };
        let arguments = self.save_window_arguments(function, &call, slots);
        let result = self.with_argument_origin(origin, |vm| {
            vm.bind_window(function, &call, slots, &extra_named)
        });
        let frame = Frame::of_kind(function, call.kind, call.base(), destination);
        self.call_stack.push(frame);
        if call.called.is_some() {
            self.call_stack.current_extras_mut().called = call.called;
        }
        if let Some(arguments) = arguments {
            self.call_stack.current_extras_mut().arguments = arguments;
        }
        result
    }

    /// The window of a prepared call to PHP code, to read its `$this`,
    /// closure and class context while its arguments are bound.
    pub(crate) fn prepared_context(
        &self,
        function: FunctionIndex,
        call: &PreparedCall,
    ) -> WindowContext<'_> {
        WindowContext {
            kind: call.kind,
            function,
            base: call.base(),
            called: call.called,
            code: &self.code,
            registers: &self.registers,
            classes: &self.classes,
        }
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

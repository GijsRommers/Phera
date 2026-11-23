//! How a native loop enters its callback: as an ordinary PHP frame on the
//! interpreter's frame stack, marked as returning to the built-in.
//!
//! A closure written in PHP is entered the way the interpreter loop enters
//! `$f($value)`: its window with the closure and its captured variables, the
//! arguments where its parameters are, and its frame. Any other callable
//! that runs as PHP code goes through the prepared-call path. What does not
//! run as an ordinary frame (a generator, a by-reference parameter) never
//! starts a native loop (see [`Vm::callback_runs_on_the_frame_stack`]).

use std::rc::Rc;

use phera_extension_api::NativeError;
use phera_value::{Object, Value};

use crate::Vm;
use crate::calls::call_state::{CallTarget, Callee};
use crate::calls::prepared_call::PreparedCallee;

impl Vm {
    /// Whether `callee` is PHP code that runs as an ordinary frame: not a
    /// generator, every parameter taken by value. Tried by preparing the
    /// call and taking it back.
    pub(super) fn callback_runs_on_the_frame_stack(&mut self, callee: &Callee) -> bool {
        let start = self.registers.len();
        if self.prepare_call(callee.clone()).is_err() {
            return false;
        }
        let runs = self.call_stack.last_call().is_some_and(|call| {
            matches!(call.callee, PreparedCallee::Php(function)
                if !self.code.function(function).is_generator
                    && self.code.facts(function).takes_values_only())
        });
        let _ = self.call_stack.pop_call();
        self.registers.truncate(start);
        runs
    }

    /// Enters the callback with `arguments`. When that fails (an argument
    /// the callback's type refuses), the built-in ends with the error.
    pub(super) fn enter_loop_callback(
        &mut self,
        arguments: Result<[Option<Value>; 2], NativeError>,
    ) -> Result<(), NativeError> {
        let entered = arguments.and_then(|arguments| self.try_enter_loop_callback(arguments));
        if entered.is_err() {
            self.abandon_native_loop();
        }
        entered
    }

    pub(super) fn try_enter_loop_callback(
        &mut self,
        arguments: [Option<Value>; 2],
    ) -> Result<(), NativeError> {
        let Some(state) = self.call_stack.native_loops.last() else {
            return Err(internal_error!("native loop"));
        };
        let origin = state.origin;
        let closure = match &state.callee.target {
            CallTarget::Closure(closure) => Some(closure.clone()),
            _ => None,
        };
        let count = arguments.iter().flatten().count();
        let mut arguments = arguments.map(Option::unwrap_or_default);
        let arguments = arguments.get_mut(..count).unwrap_or_default();
        if let Some(closure) = closure
            && self.enter_closure_callback(&closure, arguments)
        {
            return Ok(());
        }
        let Some(state) = self.call_stack.native_loops.last() else {
            return Err(internal_error!("native loop"));
        };
        self.prepare_call(state.callee.clone())?;
        for (index, argument) in arguments.iter_mut().enumerate() {
            self.store_sent_value(index, std::mem::take(argument))?;
        }
        match self.with_argument_origin(origin, |vm| vm.enter_prepared_call(None))? {
            crate::calls::call_execution::CallEntry::Php => {}
            _ => return Err(internal_error!("native loop callback")),
        }
        if let Some(frame) = self.call_stack.current_mut() {
            frame.mark_native_loop();
        }
        Ok(())
    }

    /// Enters a closure written in PHP with these arguments the way the
    /// interpreter loop enters `$f($value)`: its window with the closure
    /// and its captured variables, the arguments where its parameters are,
    /// and its frame. `false`, with nothing changed, for any shape that
    /// needs the general path (a type an argument does not have as it is, a
    /// default to fill, a bound or unbound closure, the frame limit).
    pub(super) fn enter_closure_callback(
        &mut self,
        closure: &Rc<Object>,
        arguments: &mut [Value],
    ) -> bool {
        use crate::calls::frame::Frame;
        use crate::function_facts::FunctionShape;
        let function = {
            let native = closure.native.borrow();
            let Some(state) = native
                .as_ref()
                .and_then(|state| state.downcast_ref::<crate::closures::ClosureState>())
            else {
                return false;
            };
            if state.origin != crate::closures::ClosureOrigin::Lexical {
                return false;
            }
            let Some(function) = state.callee.user_function() else {
                return false;
            };
            function
        };
        let facts = self.code.facts(function);
        let count = arguments.len();
        let fits = facts.shape == FunctionShape::Plain
            && usize::from(facts.required) <= count
            && usize::from(facts.parameters) >= count
            && arguments.iter().enumerate().all(|(index, argument)| {
                facts.takes_value(index)
                    && (!facts.is_typed(index)
                        || facts
                            .parameter_masks
                            .get(index)
                            .is_some_and(|mask| mask.intersects(crate::types::own_type(argument))))
            })
            && self.call_stack.depth() < self.frame_limit;
        if !fits {
            return false;
        }
        let parts = (&mut self.call_stack, &mut self.registers);
        if !crate::calls::php_windows::push_closure_call(&self.code, parts, closure) {
            return false;
        }
        let Some(call) = self.call_stack.pop_plain() else {
            return false;
        };
        for (offset, argument) in arguments.iter_mut().enumerate() {
            self.registers
                .put(call.base() + offset, std::mem::take(argument));
        }
        let mut frame = Frame::of_kind(function, call.kind, call.base(), None);
        if usize::from(facts.parameters) > count {
            frame = frame.with_passed(count);
        }
        frame.mark_native_loop();
        self.call_stack.push(frame);
        if call.called.is_some() {
            self.call_stack.current_extras_mut().called = call.called;
        }
        true
    }
}

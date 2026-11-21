//! Built-ins whose callbacks run on the interpreter's own frame stack.
//!
//! ```php
//! $names = array_map(fn ($user) => $user->name, $users);
//! ```
//!
//! An async built-in awaits a callback per element: a prepared call, a
//! callback window, a fresh run of the interpreter loop and a future for
//! each step, about 2,000 machine instructions before the callback does
//! anything. Here the built-in is a state instead ([`NativeLoop`], on a
//! stack in the `CallStack`): it enters its callback as an ordinary PHP
//! frame, marked as returning to the built-in, and the interpreter goes on
//! in the same loop. When that frame returns, the value goes to the state's
//! next step ([`Vm::resume_native_loop`]), which enters the next callback
//! or writes the built-in's result to the register its call returns to.
//!
//! So far `array_map` over one array with a closure that is PHP code and
//! takes its parameter by value. Everything else (a string or array
//! callable, which may need the autoloader; several arrays; a generator; a
//! by-reference parameter; named arguments; pending error handlers) takes
//! the async path as before. An exception from a callback unwinds its frame
//! and drops the state ([`Vm::abandon_native_loop`]); the built-in's frame
//! in a trace comes from the callback site pushed while it runs, as before.

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, PhpArray, Value};

use crate::Vm;
use crate::calls::call_state::Callee;
use crate::calls::callback_binding::ArgumentOrigin;
use crate::calls::execution_loop::BatchExit;
use crate::calls::prepared_call::PreparedCallee;
use crate::error_traces::NativeCaller;

/// A built-in in the middle of its callbacks.
pub(crate) struct NativeLoop {
    /// The call stack's depth while a callback of this built-in runs: the
    /// depth of the callback's frame.
    depth: usize,
    /// The register of the built-in's caller that gets the result.
    destination: Reg,
    step: Map,
}

/// `array_map` over one array: the input, where it is, and the result,
/// sized from the input up front.
struct Map {
    callee: Callee,
    origin: ArgumentOrigin,
    input: Rc<PhpArray>,
    /// The slot of the element whose callback runs, and its key.
    position: usize,
    key: ArrayKey,
    result: PhpArray,
}

impl Vm {
    /// `Call` of `array_map` as a native loop, when the call is one it
    /// takes (see the module); `None` with nothing changed otherwise.
    #[inline(never)]
    pub(crate) fn start_array_map(
        &mut self,
        destination: Reg,
    ) -> Result<Option<Option<BatchExit>>, NativeError> {
        let call = *self.call_stack.top_call()?;
        let plain = call.is_plain()
            && call.sent == 2
            && self
                .call_stack
                .top_details()
                .is_none_or(|details| details.named.is_none() && details.extra_named.is_empty())
            && self.error_handlers.pending.is_empty();
        if !plain {
            return Ok(None);
        }
        let (callback, input) = match self.registers.window(call.base(), 2) {
            [Some(callback @ Value::Object(_)), Some(Value::Array(input))] => {
                (callback.clone(), input.clone())
            }
            _ => return Ok(None),
        };
        let Ok(callee) = self.resolve_callable(&callback) else {
            return Ok(None);
        };
        if !self.callback_runs_on_the_frame_stack(&callee) {
            return Ok(None);
        }
        // The call is taken over: its arguments move to the callback site
        // that puts `array_map` in traces while its callbacks run.
        self.call_stack.pop_call()?;
        self.registers.truncate(call.start());
        let Some(first) = input.first_position() else {
            self.set(destination, Value::Array(Rc::new(PhpArray::new())));
            return Ok(Some(None));
        };
        let origin = callee.argument_binding.origin(ArgumentOrigin::Callback);
        let mut arguments = self.call_stack.native_arguments();
        arguments.extend([callback, Value::Array(input.clone())]);
        self.push_native_callback(NativeCaller::ArrayMap(arguments));
        let result = PhpArray::with_capacity(input.len());
        let state = Map {
            callee,
            origin,
            input,
            position: first,
            key: ArrayKey::Int(0),
            result,
        };
        self.call_stack.native_loops.push(NativeLoop {
            depth: self.call_stack.depth() + 1,
            destination,
            step: state,
        });
        self.enter_map_callback()?;
        Ok(Some(None))
    }

    /// Whether `callee` is PHP code that runs as an ordinary frame: not a
    /// generator, every parameter taken by value. Tried by preparing the
    /// call and taking it back.
    fn callback_runs_on_the_frame_stack(&mut self, callee: &Callee) -> bool {
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

    /// Enters the callback for the element at the state's position. When
    /// that fails (an argument the callback's type refuses), the built-in
    /// ends with the error.
    fn enter_map_callback(&mut self) -> Result<(), NativeError> {
        let entered = self.try_enter_map_callback();
        if entered.is_err() {
            self.abandon_native_loop();
        }
        entered
    }

    fn try_enter_map_callback(&mut self) -> Result<(), NativeError> {
        let Some(NativeLoop { step, .. }) = self.call_stack.native_loops.last_mut() else {
            return Err(internal_error!("native loop"));
        };
        let Some((key, element)) = step.input.entry_at(step.position) else {
            return Err(internal_error!("native loop element"));
        };
        let element = element.deref().clone();
        step.key = key;
        if let crate::calls::call_state::CallTarget::Closure(closure) = &step.callee.target {
            let closure = closure.clone();
            if self.enter_closure_callback(&closure, &element) {
                return Ok(());
            }
        }
        let Some(NativeLoop { step, .. }) = self.call_stack.native_loops.last_mut() else {
            return Err(internal_error!("native loop"));
        };
        let (callee, origin) = (step.callee.clone(), step.origin);
        self.prepare_call(callee)?;
        self.store_sent_value(0, element)?;
        match self.with_argument_origin(origin, |vm| vm.enter_prepared_call(None))? {
            crate::calls::call_execution::CallEntry::Php => {}
            _ => return Err(internal_error!("native loop callback")),
        }
        if let Some(frame) = self.call_stack.current_mut() {
            frame.mark_native_loop();
        }
        Ok(())
    }

    /// Enters a closure written in PHP with one argument the way the
    /// interpreter loop enters `$f($value)`: its window with the closure
    /// and its captured variables, the argument where its first parameter
    /// is, and its frame. `false`, with nothing changed, for any shape that
    /// needs the general path (a type the value does not have as it is, a
    /// default to fill, a bound or unbound closure, the frame limit).
    fn enter_closure_callback(
        &mut self,
        closure: &Rc<phera_value::Object>,
        element: &Value,
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
        let fits = facts.shape == FunctionShape::Plain
            && facts.required <= 1
            && facts.parameters >= 1
            && facts.takes_value(0)
            && (!facts.is_typed(0)
                || facts
                    .parameter_masks
                    .first()
                    .is_some_and(|mask| mask.intersects(crate::types::own_type(element))))
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
        self.registers.put(call.base(), element.clone());
        let mut frame = Frame::of_kind(function, call.kind, call.base(), None);
        if facts.parameters > 1 {
            frame = frame.with_passed(1);
        }
        frame.mark_native_loop();
        self.call_stack.push(frame);
        if call.called.is_some() {
            self.call_stack.current_extras_mut().called = call.called;
        }
        true
    }

    /// A callback of a native loop returned `value`: its next step.
    #[cold]
    #[inline(never)]
    pub(crate) fn resume_native_loop(&mut self, value: Value) -> Result<(), NativeError> {
        let depth = self.call_stack.depth();
        let Some(NativeLoop {
            step, depth: owner, ..
        }) = self.call_stack.native_loops.last_mut()
        else {
            return Err(internal_error!("native loop"));
        };
        if *owner != depth + 1 {
            return Err(internal_error!("native loop depth"));
        }
        step.result.insert(step.key.clone(), value);
        if let Some(next) = step.input.next_position(step.position) {
            step.position = next;
            return self.enter_map_callback();
        }
        let Some(NativeLoop {
            destination, step, ..
        }) = self.call_stack.native_loops.pop()
        else {
            return Err(internal_error!("native loop"));
        };
        self.finish_native_loop_site();
        self.set(destination, Value::Array(Rc::new(step.result)));
        Ok(())
    }

    /// An exception left the callback frame of the innermost native loop:
    /// the built-in ends with it.
    #[cold]
    #[inline(never)]
    pub(crate) fn abandon_native_loop(&mut self) {
        if self.call_stack.native_loops.pop().is_some() {
            self.finish_native_loop_site();
        }
    }

    /// Takes the built-in out of traces again.
    fn finish_native_loop_site(&mut self) {
        if let Some(crate::error_traces::NativeCallbackSite {
            native: NativeCaller::ArrayMap(arguments),
            ..
        }) = self.native_callback_sites.pop()
        {
            self.call_stack.recycle_native_arguments(arguments);
        }
    }
}

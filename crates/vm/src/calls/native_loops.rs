//! Built-ins whose callbacks run on the interpreter's own frame stack.
//!
//! ```php
//! $names = array_map(fn ($user) => $user->name, $users);
//! $active = array_filter($users, fn ($user) => $user->active);
//! $total = array_reduce($orders, fn ($sum, $order) => $sum + $order->total, 0);
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
//! or writes the built-in's result to the register its call returns to. A
//! closure written in PHP is entered the way the interpreter loop enters
//! `$f($value)`, and its plain return is made in the loop.
//!
//! So far `array_map` over one array, `array_filter` with a callback,
//! `array_reduce`, and `usort`, `uasort` and `uksort` (whose sort asks for
//! one comparison at a time, `calls::native_sorts`), each with a closure that is PHP code and takes its
//! parameters by value. Everything else (a string or array callable, which
//! may need the autoloader; several arrays; a generator; a by-reference
//! parameter; named arguments; pending error handlers) takes the async path
//! as before. An exception from a callback unwinds its frame and drops the
//! state ([`Vm::abandon_native_loop`]); the built-in's frame in a trace
//! comes from the callback site pushed while it runs, as before.

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_extension_api::{FunctionEntry, NativeError};
use phera_value::{ArrayKey, PhpArray, Value};

use crate::Vm;
use crate::calls::call_state::Callee;
use crate::calls::callback_binding::ArgumentOrigin;
use crate::calls::execution_loop::BatchExit;
use crate::calls::native_sorts::{NativeSort, SortStep, UserSort};
use crate::error_traces::NativeCaller;

/// A built-in in the middle of its callbacks.
pub(crate) struct NativeLoop {
    /// The call stack's depth while a callback of this built-in runs: the
    /// depth of the callback's frame.
    depth: usize,
    /// The register of the built-in's caller that gets the result.
    destination: Reg,
    kind: Kind,
    pub(super) callee: Callee,
    pub(super) origin: ArgumentOrigin,
    input: Rc<PhpArray>,
    /// The slot of the element whose callback runs, and its key.
    position: usize,
    key: ArrayKey,
    /// What `array_map` and `array_filter` build, sized from the input.
    result: PhpArray,
    /// What `array_reduce` carries from one callback to the next.
    carry: Value,
    /// A user sort's own state, out of the state while its step runs.
    sort: Option<Box<NativeSort>>,
}

/// Which built-in, and for `array_filter` what its callback is given.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Map,
    Filter(FilterArguments),
    Reduce,
    Sort,
}

/// `array_filter`'s mode: the value, the key (`ARRAY_FILTER_USE_KEY`) or
/// both (`ARRAY_FILTER_USE_BOTH`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum FilterArguments {
    Value,
    Key,
    Both,
}

impl Vm {
    /// `Call` of `array_map` as a native loop, when the call is one it
    /// takes (see the module); `None` with nothing changed otherwise.
    #[inline(never)]
    pub(crate) fn start_array_map(
        &mut self,
        destination: Reg,
    ) -> Result<Option<Option<BatchExit>>, NativeError> {
        self.start_native_loop(destination, None)
    }

    /// The same for `array_filter`, `array_reduce` and the user sorts, by
    /// `entry`.
    #[inline(never)]
    pub(crate) fn start_native_function_loop(
        &mut self,
        destination: Reg,
        entry: FunctionEntry,
    ) -> Result<Option<Option<BatchExit>>, NativeError> {
        self.start_native_loop(destination, Some(entry))
    }

    fn start_native_loop(
        &mut self,
        destination: Reg,
        entry: Option<FunctionEntry>,
    ) -> Result<Option<Option<BatchExit>>, NativeError> {
        let call = *self.call_stack.top_call()?;
        let plain = call.is_plain()
            && self
                .call_stack
                .top_details()
                .is_none_or(|details| details.named.is_none() && details.extra_named.is_empty())
            && self.error_handlers.pending.is_empty();
        if !plain {
            return Ok(None);
        }
        let sent = call.sent as usize;
        let window = self.registers.window(call.base(), sent);
        let name = entry.map(|entry| entry.name);
        let mut sort = None;
        let (kind, callback, input, carry) = match (name, window) {
            (None, [Some(callback @ Value::Object(_)), Some(Value::Array(input))]) => {
                (Kind::Map, callback, input.clone(), Value::Null)
            }
            (
                Some("array_filter"),
                [
                    Some(Value::Array(input)),
                    Some(callback @ Value::Object(_)),
                    mode @ ..,
                ],
            ) => {
                let arguments = match mode {
                    [] | [Some(Value::Int(0))] => FilterArguments::Value,
                    [Some(Value::Int(1))] => FilterArguments::Both,
                    [Some(Value::Int(2))] => FilterArguments::Key,
                    _ => return Ok(None),
                };
                (
                    Kind::Filter(arguments),
                    callback,
                    input.clone(),
                    Value::Null,
                )
            }
            (
                Some("array_reduce"),
                [
                    Some(Value::Array(input)),
                    Some(callback @ Value::Object(_)),
                    initial @ ..,
                ],
            ) => {
                let carry = match initial {
                    [] => Value::Null,
                    [Some(initial)] if !initial.is_reference() => initial.clone(),
                    _ => return Ok(None),
                };
                (Kind::Reduce, callback, input.clone(), carry)
            }
            (
                Some(name),
                [
                    Some(Value::Reference(variable)),
                    Some(callback @ Value::Object(_)),
                ],
            ) => {
                let Some(which) = UserSort::of(name) else {
                    return Ok(None);
                };
                let Ok(held) = variable.try_borrow() else {
                    return Ok(None);
                };
                let Value::Array(input) = &*held else {
                    return Ok(None);
                };
                let input = input.clone();
                drop(held);
                sort = Some((which, variable.clone()));
                (Kind::Sort, callback, input, Value::Null)
            }
            _ => return Ok(None),
        };
        let callback = callback.clone();
        let Ok(callee) = self.resolve_callable(&callback) else {
            return Ok(None);
        };
        if !self.callback_runs_on_the_frame_stack(&callee) {
            return Ok(None);
        }
        // The call is taken over: its arguments move to the callback site
        // that puts the built-in in traces while its callbacks run.
        let mut arguments = self.call_stack.native_arguments();
        for slot in self.registers.window_mut(call.base(), sent) {
            arguments.push(slot.take().unwrap_or_default());
        }
        self.call_stack.pop_call()?;
        self.registers.truncate(call.start());
        let Some(first) = input.first_position() else {
            let result = match kind {
                Kind::Reduce => carry,
                Kind::Map | Kind::Filter(_) => Value::Array(Rc::new(PhpArray::new())),
                Kind::Sort => Value::Bool(true),
            };
            self.call_stack.recycle_native_arguments(arguments);
            self.set_call_result(destination, result);
            return Ok(Some(None));
        };
        let site = match entry {
            None => NativeCaller::ArrayMap(arguments),
            Some(entry) => {
                let trace = self.native_function_trace(entry.name, &arguments);
                self.call_stack.recycle_native_arguments(arguments);
                NativeCaller::Saved(trace)
            }
        };
        self.push_native_callback(site);
        let origin = callee.argument_binding.origin(ArgumentOrigin::Callback);
        let result = match kind {
            Kind::Map => PhpArray::with_capacity(input.len()),
            Kind::Filter(_) | Kind::Reduce | Kind::Sort => PhpArray::new(),
        };
        let sort = sort.map(|(which, variable)| NativeSort::new(which, &input, variable));
        self.call_stack.native_loops.push(NativeLoop {
            depth: self.call_stack.depth() + 1,
            destination,
            kind,
            callee,
            origin,
            input,
            position: first,
            key: ArrayKey::Int(0),
            result,
            carry,
            sort: sort.map(Box::new),
        });
        match kind {
            Kind::Sort => self.sort_step(None)?,
            Kind::Map | Kind::Filter(_) | Kind::Reduce => self.enter_element_callback()?,
        }
        Ok(Some(None))
    }

    /// Enters the callback for the element at the state's position.
    fn enter_element_callback(&mut self) -> Result<(), NativeError> {
        let arguments = self.element_arguments();
        self.enter_loop_callback(arguments)
    }

    /// The callback's arguments for the element at the state's position.
    fn element_arguments(&mut self) -> Result<[Option<Value>; 2], NativeError> {
        let Some(state) = self.call_stack.native_loops.last_mut() else {
            return Err(internal_error!("native loop"));
        };
        let Some((key, element)) = state.input.entry_at(state.position) else {
            return Err(internal_error!("native loop element"));
        };
        let element = element.deref().clone();
        let arguments = match state.kind {
            Kind::Map | Kind::Filter(FilterArguments::Value) => [Some(element), None],
            Kind::Filter(FilterArguments::Key) => [Some(Value::from(key.clone())), None],
            Kind::Filter(FilterArguments::Both) => [Some(element), Some(Value::from(key.clone()))],
            Kind::Reduce => [Some(std::mem::take(&mut state.carry)), Some(element)],
            Kind::Sort => return Err(internal_error!("native loop sort")),
        };
        state.key = key;
        Ok(arguments)
    }

    /// A callback of a native loop returned `value`: its next step.
    #[cold]
    #[inline(never)]
    pub(crate) fn resume_native_loop(&mut self, value: Value) -> Result<(), NativeError> {
        let depth = self.call_stack.depth();
        let nan = matches!(&value, Value::Float(float) if float.is_nan());
        let Some(state) = self.call_stack.native_loops.last_mut() else {
            return Err(internal_error!("native loop"));
        };
        if state.depth != depth + 1 {
            return Err(internal_error!("native loop depth"));
        }
        let filter = matches!(state.kind, Kind::Filter(_));
        match state.kind {
            Kind::Map => {
                state.result.insert(state.key.clone(), value);
            }
            Kind::Filter(_) => {
                if phera_value::to_bool(&value)
                    && let Some((key, element)) = state.input.entry_at(state.position)
                {
                    state.result.insert(key, element.clone());
                }
            }
            Kind::Reduce => state.carry = value.deref().clone(),
            Kind::Sort => return self.sort_step(Some(value)),
        }
        let next = state.input.next_position(state.position);
        if nan && filter {
            self.warn("unexpected NAN value was coerced to bool".into());
        }
        if let Some(next) = next {
            if let Some(state) = self.call_stack.native_loops.last_mut() {
                state.position = next;
            }
            return self.enter_element_callback();
        }
        let Some(state) = self.call_stack.native_loops.pop() else {
            return Err(internal_error!("native loop"));
        };
        self.finish_native_loop_site();
        let result = match state.kind {
            Kind::Reduce => state.carry,
            Kind::Map | Kind::Filter(_) => Value::Array(Rc::new(state.result)),
            Kind::Sort => Value::Bool(true),
        };
        self.set(state.destination, result);
        Ok(())
    }

    /// The next step of a user sort: with the value its last callback
    /// returned, if any, the sort runs to its next comparison and enters
    /// the callback for it, or it is done and the call returns `true`.
    fn sort_step(&mut self, returned: Option<Value>) -> Result<(), NativeError> {
        let Some(mut sort) = self
            .call_stack
            .native_loops
            .last_mut()
            .and_then(|state| state.sort.take())
        else {
            return Err(internal_error!("native loop sort"));
        };
        let again = returned.and_then(|value| sort.answered(&value.deref().clone(), self));
        let step = match again {
            Some(arguments) => SortStep::Compare(arguments),
            None => sort.next_step(),
        };
        match step {
            SortStep::Compare([left, right]) => {
                if let Some(state) = self.call_stack.native_loops.last_mut() {
                    state.sort = Some(sort);
                }
                self.enter_loop_callback(Ok([Some(left), Some(right)]))
            }
            SortStep::Done(result) => {
                let Some(state) = self.call_stack.native_loops.pop() else {
                    return Err(internal_error!("native loop"));
                };
                self.finish_native_loop_site();
                result?;
                self.set(state.destination, Value::Bool(true));
                Ok(())
            }
        }
    }

    /// An exception left the callback frame of the innermost native loop:
    /// the built-in ends with it.
    #[cold]
    #[inline(never)]
    pub(crate) fn abandon_native_loop(&mut self) {
        if let Some(state) = self.call_stack.native_loops.pop() {
            if let Some(sort) = state.sort {
                sort.abandon();
            }
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

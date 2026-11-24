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
    /// The slot of the element whose callback runs (`None` once `input` is
    /// exhausted in a two-array `array_map`), and its key.
    position: Option<usize>,
    /// Second array and cursor for two-array `array_map($fn, $a, $b)`.
    second: Option<(Rc<PhpArray>, Option<usize>)>,
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
    Map2,
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
        let (kind, callback, input, second, carry) = match (name, window) {
            (None, [Some(callback), Some(Value::Array(input))])
                if callable_ready_for_loop(callback) =>
            {
                (Kind::Map, callback, input.clone(), None, Value::Null)
            }
            (
                None,
                [
                    Some(callback),
                    Some(Value::Array(first)),
                    Some(Value::Array(second)),
                ],
            ) if callable_ready_for_loop(callback) => (
                Kind::Map2,
                callback,
                first.clone(),
                Some(second.clone()),
                Value::Null,
            ),
            (Some("array_filter"), [Some(Value::Array(input)), Some(callback), mode @ ..])
                if callable_ready_for_loop(callback) =>
            {
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
                    None,
                    Value::Null,
                )
            }
            (Some("array_reduce"), [Some(Value::Array(input)), Some(callback), initial @ ..])
                if callable_ready_for_loop(callback) =>
            {
                let carry = match initial {
                    [] => Value::Null,
                    [Some(initial)] if !initial.is_reference() => initial.clone(),
                    _ => return Ok(None),
                };
                (Kind::Reduce, callback, input.clone(), None, carry)
            }
            (Some(name), [Some(Value::Reference(variable)), Some(callback)])
                if callable_ready_for_loop(callback) =>
            {
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
                (Kind::Sort, callback, input, None, Value::Null)
            }
            _ => return Ok(None),
        };
        let callback = callback.clone();
        let Ok(callee) = self.resolve_callable(&callback) else {
            return Ok(None);
        };
        if matches!(kind, Kind::Map | Kind::Map2)
            && let Some(exit) =
                self.try_sync_native_array_map(&callee, &input, second.as_ref(), destination, call)?
        {
            return Ok(Some(exit));
        }
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
        let first = input.first_position();
        let second_first = second.as_ref().and_then(|array| array.first_position());
        let empty = match kind {
            Kind::Map2 => first.is_none() && second_first.is_none(),
            _ => first.is_none(),
        };
        if empty {
            let result = match kind {
                Kind::Reduce => carry,
                Kind::Map | Kind::Map2 | Kind::Filter(_) => Value::Array(Rc::new(PhpArray::new())),
                Kind::Sort => Value::Bool(true),
            };
            self.call_stack.recycle_native_arguments(arguments);
            self.set(destination, result);
            return Ok(Some(None));
        }
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
            Kind::Map2 => PhpArray::with_capacity(
                input
                    .len()
                    .max(second.as_ref().map_or(0, |array| array.len())),
            ),
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
            second: second.map(|array| (array, second_first)),
            key: ArrayKey::Int(0),
            result,
            carry,
            sort: sort.map(Box::new),
        });
        match kind {
            Kind::Sort => self.sort_step(None)?,
            Kind::Map | Kind::Map2 | Kind::Filter(_) | Kind::Reduce => {
                self.enter_element_callback()?
            }
        }
        Ok(Some(None))
    }

    /// Runs `array_map` over 1 or 2 arrays synchronously when the callback is a
    /// plain synchronous native function (e.g. `strtolower`, `trim`, `intval`)
    /// and every element matches its parameter types without coercion or error
    /// handlers.
    fn try_sync_native_array_map(
        &mut self,
        callee: &Callee,
        first: &Rc<PhpArray>,
        second: Option<&Rc<PhpArray>>,
        destination: Reg,
        call: crate::calls::prepared_call::PreparedCall,
    ) -> Result<Option<Option<BatchExit>>, NativeError> {
        use phera_extension_api::NativeFn;
        let crate::calls::call_state::CallTarget::Native(entry) = callee.target else {
            return Ok(None);
        };
        let NativeFn::Sync(handler) = entry.handler else {
            return Ok(None);
        };
        if (self.error_handlers.error.is_some() && !is_infallible_sync_callback(entry.name))
            || self.output_writer.buffers.pending.is_some()
            || self.registry.object_handler(entry.name).is_some()
            || entry.params.iter().any(|param| param.by_ref)
        {
            return Ok(None);
        }
        let arity = if second.is_some() { 2 } else { 1 };
        // Pre-validate every row against the sync signature so any coercion,
        // object handler, or argument-count mismatch stays on the general path.
        let required = entry.params.iter().filter(|param| !param.optional).count();
        if arity < required
            || (arity > entry.params.len() && !entry.params.iter().any(|p| p.variadic))
        {
            return Ok(None);
        }
        match second {
            None => {
                for (_, value) in first.iter() {
                    if value.is_reference()
                        || matches!(value, Value::Object(_))
                        || !crate::calls::native_signature::accepts(
                            entry.params,
                            &[Some(value.clone())],
                        )
                    {
                        return Ok(None);
                    }
                }
            }
            Some(second) => {
                let len = first.len().max(second.len());
                let mut left = first.iter();
                let mut right = second.iter();
                for _ in 0..len {
                    let a = left.next().map(|(_, v)| v).unwrap_or(&Value::Null);
                    let b = right.next().map(|(_, v)| v).unwrap_or(&Value::Null);
                    if a.is_reference()
                        || b.is_reference()
                        || matches!(a, Value::Object(_))
                        || matches!(b, Value::Object(_))
                        || !crate::calls::native_signature::accepts(
                            entry.params,
                            &[Some(a.clone()), Some(b.clone())],
                        )
                    {
                        return Ok(None);
                    }
                }
            }
        }
        // Take over the prepared call and push the array_map trace site.
        let sent = call.sent as usize;
        let mut call_args = self.call_stack.native_arguments();
        for slot in self.registers.window_mut(call.base(), sent) {
            call_args.push(slot.take().unwrap_or_default());
        }
        self.call_stack.pop_call()?;
        self.registers.truncate(call.start());
        self.push_native_callback(NativeCaller::ArrayMap(call_args));

        let len = first.len().max(second.map_or(0, |s| s.len()));
        let mut result = PhpArray::with_capacity(len);
        let outcome: Result<(), NativeError> = match second {
            None => {
                let mut err = None;
                for (key, value) in first.iter() {
                    let arg = [value.clone()];
                    let previous_class = self.native_called_class.take();
                    let previous_view = self.native_view_depth.replace(self.call_stack.depth());
                    let mapped = handler(self, &arg);
                    self.native_view_depth = previous_view;
                    self.native_called_class = previous_class;
                    match mapped {
                        Ok(mapped) => {
                            result.insert(key, mapped);
                        }
                        Err(error) => {
                            self.record_native_failure(
                                &error,
                                crate::calls::native_frames::NativeFrame::Function(entry.name),
                                &arg,
                            );
                            err = Some(error);
                            break;
                        }
                    }
                }
                err.map_or(Ok(()), Err)
            }
            Some(second) => {
                let mut left = first.iter();
                let mut right = second.iter();
                let mut err = None;
                for _ in 0..len {
                    let a = left.next().map(|(_, v)| v.clone()).unwrap_or_default();
                    let b = right.next().map(|(_, v)| v.clone()).unwrap_or_default();
                    let args = [a, b];
                    let previous_class = self.native_called_class.take();
                    let previous_view = self.native_view_depth.replace(self.call_stack.depth());
                    let mapped = handler(self, &args);
                    self.native_view_depth = previous_view;
                    self.native_called_class = previous_class;
                    match mapped {
                        Ok(mapped) => {
                            if let Err(error) = result.push(mapped) {
                                err = Some(error.into());
                                break;
                            }
                        }
                        Err(error) => {
                            self.record_native_failure(
                                &error,
                                crate::calls::native_frames::NativeFrame::Function(entry.name),
                                &args,
                            );
                            err = Some(error);
                            break;
                        }
                    }
                }
                err.map_or(Ok(()), Err)
            }
        };
        if let Some(crate::error_traces::NativeCallbackSite {
            native: NativeCaller::ArrayMap(arguments),
            ..
        }) = self.native_callback_sites.pop()
        {
            if let Err(error) = &outcome {
                self.record_native_failure(
                    error,
                    crate::calls::native_frames::NativeFrame::Function("array_map"),
                    &arguments,
                );
            }
            self.call_stack.recycle_native_arguments(arguments);
        }
        outcome?;
        self.set(destination, Value::Array(Rc::new(result)));
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
        if state.kind == Kind::Map2 {
            let first = state
                .position
                .and_then(|pos| state.input.entry_at(pos))
                .map(|(_, value)| value.deref().clone())
                .unwrap_or_default();
            let second = state
                .second
                .as_ref()
                .and_then(|(array, pos)| pos.and_then(|pos| array.entry_at(pos)))
                .map(|(_, value)| value.deref().clone())
                .unwrap_or_default();
            return Ok([Some(first), Some(second)]);
        }
        let Some(pos) = state.position else {
            return Err(internal_error!("native loop element"));
        };
        let Some((key, element)) = state.input.entry_at(pos) else {
            return Err(internal_error!("native loop element"));
        };
        let element = element.deref().clone();
        let arguments = match state.kind {
            Kind::Map | Kind::Filter(FilterArguments::Value) => [Some(element), None],
            Kind::Filter(FilterArguments::Key) => [Some(Value::from(key.clone())), None],
            Kind::Filter(FilterArguments::Both) => [Some(element), Some(Value::from(key.clone()))],
            Kind::Reduce => [Some(std::mem::take(&mut state.carry)), Some(element)],
            Kind::Map2 | Kind::Sort => return Err(internal_error!("native loop sort")),
        };
        state.key = key;
        Ok(arguments)
    }

    /// A callback of a native loop returned `value`: its next step.
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
        let has_next = match state.kind {
            Kind::Map => {
                state.result.insert(state.key.clone(), value);
                state.position = state
                    .position
                    .and_then(|pos| state.input.next_position(pos));
                state.position.is_some()
            }
            Kind::Map2 => {
                state.result.push(value)?;
                state.position = state
                    .position
                    .and_then(|pos| state.input.next_position(pos));
                if let Some((array, pos)) = &mut state.second {
                    *pos = pos.and_then(|p| array.next_position(p));
                }
                state.position.is_some()
                    || state.second.as_ref().is_some_and(|(_, pos)| pos.is_some())
            }
            Kind::Filter(_) => {
                if phera_value::to_bool(&value)
                    && let Some(pos) = state.position
                    && let Some((key, element)) = state.input.entry_at(pos)
                {
                    state.result.insert(key, element.clone());
                }
                state.position = state
                    .position
                    .and_then(|pos| state.input.next_position(pos));
                state.position.is_some()
            }
            Kind::Reduce => {
                state.carry = value.deref().clone();
                state.position = state
                    .position
                    .and_then(|pos| state.input.next_position(pos));
                state.position.is_some()
            }
            Kind::Sort => return self.sort_step(Some(value)),
        };
        if nan && filter {
            self.warn("unexpected NAN value was coerced to bool".into());
        }
        if has_next {
            return self.enter_element_callback();
        }
        let Some(state) = self.call_stack.native_loops.pop() else {
            return Err(internal_error!("native loop"));
        };
        self.finish_native_loop_site();
        let result = match state.kind {
            Kind::Reduce => state.carry,
            Kind::Map | Kind::Map2 | Kind::Filter(_) => Value::Array(Rc::new(state.result)),
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

/// Continues an active `array_map`/`array_filter`/`array_reduce` loop directly
/// inside `Machine::run` when there is a next element whose callback enters via
/// the direct PHP/closure fast path. Returns `None` with nothing mutated on the
/// final element, on `Sort`, on a NaN filter verdict, or when the next element
/// needs general coercion/binding.
pub(crate) fn try_continue_in_machine(
    code: &crate::code_table::CodeTable,
    classes: &crate::classes::ClassTable,
    stack: &mut crate::calls::call_stack::CallStack,
    registers: &mut crate::calls::registers::Registers,
    frame_limit: usize,
    value: &mut Value,
) -> Option<crate::calls::frame::Frame> {
    let depth = stack.depth();
    if depth >= frame_limit {
        return None;
    }
    let nan = matches!(value, Value::Float(float) if float.is_nan());
    let (kind, next_pos, next_second_pos, next_key, mut next_args, arg_count, target) = {
        let state = stack.native_loops.last()?;
        if state.depth != depth + 1 || state.kind == Kind::Sort {
            return None;
        }
        if nan && matches!(state.kind, Kind::Filter(_)) {
            return None;
        }
        let next_pos = state
            .position
            .and_then(|pos| state.input.next_position(pos));
        let next_second_pos = if state.kind == Kind::Map2 {
            state
                .second
                .as_ref()
                .and_then(|(array, pos)| pos.and_then(|p| array.next_position(p)))
        } else {
            None
        };
        let has_next = match state.kind {
            Kind::Map2 => next_pos.is_some() || next_second_pos.is_some(),
            _ => next_pos.is_some(),
        };
        if !has_next {
            return None;
        }
        let (next_key, args, count) = match state.kind {
            Kind::Map2 => {
                let first = next_pos
                    .and_then(|pos| state.input.entry_at(pos))
                    .map(|(_, v)| v.deref().clone())
                    .unwrap_or_default();
                let second = next_second_pos
                    .and_then(|pos| state.second.as_ref()?.0.entry_at(pos))
                    .map(|(_, v)| v.deref().clone())
                    .unwrap_or_default();
                (ArrayKey::Int(0), [first, second], 2)
            }
            Kind::Map | Kind::Filter(FilterArguments::Value) => {
                let (key, elem) = state.input.entry_at(next_pos?)?;
                (key, [elem.deref().clone(), Value::Null], 1)
            }
            Kind::Filter(FilterArguments::Key) => {
                let (key, _) = state.input.entry_at(next_pos?)?;
                let key_val = Value::from(key.clone());
                (key, [key_val, Value::Null], 1)
            }
            Kind::Filter(FilterArguments::Both) => {
                let (key, elem) = state.input.entry_at(next_pos?)?;
                let key_val = Value::from(key.clone());
                (key, [elem.deref().clone(), key_val], 2)
            }
            Kind::Reduce => {
                let (key, elem) = state.input.entry_at(next_pos?)?;
                (key, [value.deref().clone(), elem.deref().clone()], 2)
            }
            Kind::Sort => return None,
        };
        (
            state.kind,
            next_pos,
            next_second_pos,
            next_key,
            args,
            count,
            state.callee.target.clone(),
        )
    };
    let mut env = crate::calls::native_loop_callbacks::DirectLoopEnv {
        code,
        classes,
        stack,
        registers,
        frame_limit,
    };
    let next_frame = crate::calls::native_loop_callbacks::try_enter_direct_target(
        &mut env,
        &target,
        &mut next_args[..arg_count],
    )?;
    // Direct entry succeeded; now commit the completed iteration and advance.
    let state = stack.native_loops.last_mut()?;
    match kind {
        Kind::Map => {
            state
                .result
                .insert(state.key.clone(), std::mem::take(value));
        }
        Kind::Map2 => {
            let _ = state.result.push(std::mem::take(value));
            if let Some((_, pos)) = &mut state.second {
                *pos = next_second_pos;
            }
        }
        Kind::Filter(_) => {
            if phera_value::to_bool(value)
                && let Some(pos) = state.position
                && let Some((key, element)) = state.input.entry_at(pos)
            {
                state.result.insert(key, element.clone());
            }
            *value = Value::Null;
        }
        Kind::Reduce => {
            *value = Value::Null;
        }
        Kind::Sort => {}
    }
    state.position = next_pos;
    state.key = next_key;
    Some(next_frame)
}

/// Synchronous built-ins that never emit warnings or invoke user callbacks for
/// already-type-checked non-object scalar/array arguments, so they may run in
/// one tight loop even while a PHP error handler is installed.
fn is_infallible_sync_callback(name: &str) -> bool {
    matches!(
        name,
        "strtolower"
            | "strtoupper"
            | "trim"
            | "ltrim"
            | "rtrim"
            | "strlen"
            | "strval"
            | "intval"
            | "floatval"
            | "boolval"
            | "ucfirst"
            | "lcfirst"
            | "rawurlencode"
            | "rawurldecode"
            | "urlencode"
            | "urldecode"
            | "base64_encode"
            | "md5"
            | "sha1"
            | "crc32"
            | "bin2hex"
            | "stripslashes"
            | "addslashes"
            | "nl2br"
            | "strip_tags"
            | "array_values"
            | "array_keys"
            | "count"
            | "is_string"
            | "is_int"
            | "is_array"
            | "is_null"
            | "is_bool"
            | "is_float"
            | "is_numeric"
            | "gettype"
            | "get_debug_type"
            | "basename"
            | "dirname"
            | "ord"
            | "abs"
            | "ceil"
            | "floor"
            | "dechex"
            | "strrev"
            | "preg_quote"
    )
}

/// Whether `callback` can be resolved synchronously without invoking an
/// autoloader or emitting legacy `Class::method` array-callback deprecations.
fn callable_ready_for_loop(callback: &Value) -> bool {
    match callback {
        Value::Object(_) => true,
        Value::InternedStr(string) => !string.as_bytes().windows(2).any(|pair| pair == b"::"),
        Value::String(string) => !string.as_bytes().windows(2).any(|pair| pair == b"::"),
        Value::Array(array) => {
            array.len() == 2
                && matches!(array.get(&ArrayKey::Int(0)), Some(Value::Object(_)))
                && match array.get(&ArrayKey::Int(1)) {
                    Some(Value::InternedStr(method)) => {
                        !method.as_bytes().windows(2).any(|pair| pair == b"::")
                    }
                    Some(Value::String(method)) => {
                        !method.as_bytes().windows(2).any(|pair| pair == b"::")
                    }
                    _ => false,
                }
        }
        _ => false,
    }
}

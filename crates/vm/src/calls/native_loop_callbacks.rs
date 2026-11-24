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
use crate::calls::call_stack::CallStack;
use crate::calls::call_state::{CallTarget, Callee};
use crate::calls::frame::Frame;
use crate::calls::prepared_call::CallKind;
use crate::calls::registers::Registers;
use crate::classes::{ClassIndex, ClassTable, Invocation, MethodBody};
use crate::code_table::{CodeTable, FunctionIndex};

/// Borrowed VM/Machine slices needed to push a direct native-loop callback frame.
pub(super) struct DirectLoopEnv<'a> {
    pub(super) code: &'a CodeTable,
    pub(super) classes: &'a ClassTable,
    pub(super) stack: &'a mut CallStack,
    pub(super) registers: &'a mut Registers,
    pub(super) frame_limit: usize,
}

/// Target metadata for a plain PHP function or method callback.
pub(super) struct PhpCallbackTarget {
    pub(super) function: FunctionIndex,
    pub(super) kind: CallKind,
    pub(super) receiver: Option<Rc<Object>>,
    pub(super) called: Option<ClassIndex>,
}

/// Whether `arguments` match a plain PHP function/method/closure directly
/// without coercion, defaults, or reference binding.
pub(super) fn callback_arguments_fit_direct(
    env: &DirectLoopEnv<'_>,
    function: FunctionIndex,
    arguments: &[Value],
) -> bool {
    use crate::function_facts::FunctionShape;
    let facts = env.code.facts(function);
    let count = arguments.len();
    facts.shape == FunctionShape::Plain
        && usize::from(facts.required) <= count
        && usize::from(facts.parameters) >= count
        && env.stack.depth() < env.frame_limit
        && arguments.iter().enumerate().all(|(index, argument)| {
            facts.takes_value(index)
                && (!facts.is_typed(index)
                    || facts
                        .parameter_masks
                        .get(index)
                        .is_some_and(|mask| mask.intersects(crate::types::own_type(argument)))
                    || env
                        .code
                        .function(function)
                        .parameters
                        .get(index)
                        .and_then(|param| param.ty)
                        .is_some_and(|declared| {
                            crate::class_types::fast_class_type_matches(
                                env.code,
                                env.classes,
                                function,
                                declared,
                                argument,
                            )
                        }))
        })
}

/// Direct frame entry for a PHP function or method callback (e.g.
/// `array_map([$this, 'hydrate'], $rows)`), usable from both `Vm` and `Machine`.
pub(super) fn enter_php_callback_direct(
    env: &mut DirectLoopEnv<'_>,
    target: PhpCallbackTarget,
    arguments: &mut [Value],
) -> Option<Frame> {
    if !callback_arguments_fit_direct(env, target.function, arguments) {
        return None;
    }
    let facts = env.code.facts(target.function);
    let count = arguments.len();
    let start = env.registers.len();
    let base = start + target.kind.prefix();
    let window = (facts.registers.max(facts.parameters) as usize).max(1);
    env.registers.grow_to(base + window);
    if let Some(receiver) = target.receiver {
        env.registers.put(start, Value::Object(receiver));
    }
    for (offset, argument) in arguments.iter_mut().enumerate() {
        env.registers.put(base + offset, std::mem::take(argument));
    }
    let mut frame = Frame::of_kind(target.function, target.kind, base, None);
    if usize::from(facts.parameters) > count {
        frame = frame.with_passed(count);
    }
    frame.mark_native_loop();
    env.stack.push(frame);
    if let Some(called) = target.called
        && Some(called) != facts.class
    {
        env.stack.current_extras_mut().called = Some(called);
    }
    Some(frame)
}

/// Direct single-borrow frame entry for a lexical PHP closure callback,
/// without pushing or popping a temporary `PreparedCall`.
pub(super) fn enter_closure_callback_direct(
    env: &mut DirectLoopEnv<'_>,
    closure: &Rc<Object>,
    arguments: &mut [Value],
) -> Option<Frame> {
    let native = closure.native.borrow();
    let state = native
        .as_ref()
        .and_then(|state| state.downcast_ref::<crate::closures::ClosureState>())?;
    if state.origin != crate::closures::ClosureOrigin::Lexical {
        return None;
    }
    let function = state.callee.user_function()?;
    if !callback_arguments_fit_direct(env, function, arguments) {
        return None;
    }
    let facts = env.code.facts(function);
    let count = arguments.len();
    let start = env.registers.len();
    let base = start + CallKind::Closure.prefix();
    let window = crate::calls::pending::php_window(env.code, function);
    env.registers.grow_to(base + window);
    env.registers.put(start, Value::Object(closure.clone()));
    if let Some(receiver) = state
        .context
        .as_ref()
        .and_then(|context| context.receiver.clone())
    {
        env.registers.put(start + 1, Value::Object(receiver));
    }
    for (offset, argument) in arguments.iter_mut().enumerate() {
        env.registers.put(base + offset, std::mem::take(argument));
    }
    let captures = base + env.code.function(function).parameters.len();
    for (index, value) in state.captures.iter().enumerate() {
        if let Some(value) = value {
            env.registers.put(captures + index, value.clone());
        }
    }
    let mut frame = Frame::of_kind(function, CallKind::Closure, base, None);
    if usize::from(facts.parameters) > count {
        frame = frame.with_passed(count);
    }
    frame.mark_native_loop();
    env.stack.push(frame);
    Some(frame)
}

/// Direct entry for any loop-eligible PHP target (`Closure`, `User`, `Method`),
/// returning the pushed `Frame` on success or `None` with nothing mutated.
pub(super) fn try_enter_direct_target(
    env: &mut DirectLoopEnv<'_>,
    target: &CallTarget,
    arguments: &mut [Value],
) -> Option<Frame> {
    match target {
        CallTarget::Closure(closure) => enter_closure_callback_direct(env, closure, arguments),
        CallTarget::User(function) => enter_php_callback_direct(
            env,
            PhpCallbackTarget {
                function: *function,
                kind: CallKind::Function,
                receiver: None,
                called: None,
            },
            arguments,
        ),
        CallTarget::Method { method, context } => {
            let MethodBody::User(function) = method.body else {
                return None;
            };
            if context.invocation != Invocation::Method {
                return None;
            }
            let receiver = context.receiver.clone();
            let called = receiver.is_none().then_some(context.called.index);
            enter_php_callback_direct(
                env,
                PhpCallbackTarget {
                    function,
                    kind: CallKind::Method,
                    receiver,
                    called,
                },
                arguments,
            )
        }
        _ => None,
    }
}

impl Vm {
    /// Whether `callee` is PHP code that runs as an ordinary frame: not a
    /// generator, every parameter taken by value. Inspected directly without
    /// cloning `Callee` or touching the register/call stacks.
    pub(super) fn callback_runs_on_the_frame_stack(&self, callee: &Callee) -> bool {
        let function = match &callee.target {
            CallTarget::User(function) => Some(*function),
            CallTarget::Method { method, .. } => match method.body {
                MethodBody::User(function) => Some(function),
                _ => None,
            },
            CallTarget::Closure(object) => object
                .native
                .borrow()
                .as_ref()
                .and_then(|state| state.downcast_ref::<crate::closures::ClosureState>())
                .and_then(|state| state.callee.user_function()),
            _ => None,
        };
        function.is_some_and(|function| {
            !self.code.function(function).is_generator
                && self.code.facts(function).takes_values_only()
        })
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
        let count = arguments.iter().flatten().count();
        let mut arguments = arguments.map(Option::unwrap_or_default);
        let arguments = arguments.get_mut(..count).unwrap_or_default();
        let target = state.callee.target.clone();
        let mut env = DirectLoopEnv {
            code: &self.code,
            classes: &self.classes,
            stack: &mut self.call_stack,
            registers: &mut self.registers,
            frame_limit: self.frame_limit,
        };
        if try_enter_direct_target(&mut env, &target, arguments).is_some() {
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
}
